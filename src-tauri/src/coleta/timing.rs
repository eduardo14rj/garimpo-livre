use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::Mutex as AsyncMutex;

#[async_trait]
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
    async fn sleep(&self, duration: Duration);
}

#[derive(Clone, Default)]
pub struct RelogioSistema;

#[async_trait]
impl Clock for RelogioSistema {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64
    }
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    clock: Arc<dyn Clock>,
    interval_ms: Arc<AtomicU64>,
    last_request_ms: Arc<AsyncMutex<Option<u64>>>,
}

impl RateLimiter {
    pub fn new(clock: Arc<dyn Clock>, interval: Duration) -> Self {
        Self {
            clock,
            interval_ms: Arc::new(AtomicU64::new(
                interval.as_millis().min(u64::MAX as u128) as u64
            )),
            last_request_ms: Arc::new(AsyncMutex::new(None)),
        }
    }

    pub fn set_interval(&self, interval: Duration) {
        self.interval_ms.store(
            interval.as_millis().min(u64::MAX as u128) as u64,
            Ordering::SeqCst,
        );
    }

    pub async fn wait_turn(&self) {
        let mut last = self.last_request_ms.lock().await;
        if let Some(previous) = *last {
            let elapsed = self.clock.now_ms().saturating_sub(previous);
            let interval_ms = self.interval_ms.load(Ordering::SeqCst);
            if elapsed < interval_ms {
                self.clock
                    .sleep(Duration::from_millis(interval_ms - elapsed))
                    .await;
            }
        }
        *last = Some(self.clock.now_ms());
    }
}

pub async fn retry_com_backoff<T, E, F, Fut>(
    clock: &dyn Clock,
    max_tentativas: usize,
    atraso_inicial: Duration,
    mut tentativa: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send,
    E: Send,
{
    assert!(max_tentativas > 0, "deve haver ao menos uma tentativa");
    let mut atraso = atraso_inicial;
    for numero in 1..=max_tentativas {
        match tentativa().await {
            Ok(valor) => return Ok(valor),
            Err(erro) if numero == max_tentativas => return Err(erro),
            Err(_) => {
                let jitter_limite = (atraso.as_millis() / 4).min(u64::MAX as u128) as u64;
                let jitter_ms = if jitter_limite == 0 {
                    0
                } else {
                    fastrand::u64(0..=jitter_limite)
                };
                clock.sleep(atraso + Duration::from_millis(jitter_ms)).await;
                atraso = atraso.saturating_mul(2);
            }
        }
    }
    unreachable!("o último erro já foi retornado")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };

    #[derive(Default)]
    struct RelogioFake(AtomicU64);

    #[async_trait]
    impl Clock for RelogioFake {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        async fn sleep(&self, duration: Duration) {
            self.0
                .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn limite_aguarda_dois_segundos_entre_chamadas() {
        let clock = Arc::new(RelogioFake::default());
        let limiter = RateLimiter::new(clock.clone(), Duration::from_secs(2));
        let mut marks = Vec::new();
        for _ in 0..3 {
            limiter.wait_turn().await;
            marks.push(clock.now_ms());
        }
        assert_eq!(marks, vec![0, 2_000, 4_000]);
    }

    #[tokio::test]
    async fn retry_sucede_na_terceira_tentativa_e_para_na_quinta() {
        let clock = RelogioFake::default();
        let calls = Mutex::new(0usize);
        let result = retry_com_backoff(&clock, 5, Duration::from_secs(1), || {
            let current = {
                let mut count = calls.lock().unwrap();
                *count += 1;
                *count
            };
            async move {
                if current < 3 {
                    Err("falha")
                } else {
                    Ok(current)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 3);
        assert_eq!(*calls.lock().unwrap(), 3);

        let calls = Mutex::new(0usize);
        let result: Result<(), &str> = retry_com_backoff(&clock, 5, Duration::from_secs(1), || {
            *calls.lock().unwrap() += 1;
            async { Err("falha") }
        })
        .await;
        assert_eq!(result, Err("falha"));
        assert_eq!(*calls.lock().unwrap(), 5);
    }
}
