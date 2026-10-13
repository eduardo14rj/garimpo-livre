import { connect } from "node:net";

function loopbackListening(host, port, family) {
  return new Promise((resolve) => {
    const socket = connect({ host, port, family });
    socket.once("connect", () => {
      socket.destroy();
      resolve(true);
    });
    socket.once("error", (error) => {
      socket.destroy();
      resolve(!["ECONNREFUSED", "EHOSTUNREACH", "EADDRNOTAVAIL", "EAFNOSUPPORT", "ENETUNREACH"].includes(error.code));
    });
  });
}

export async function portAvailable(port) {
  const [ipv4, ipv6] = await Promise.all([
    loopbackListening("127.0.0.1", port, 4),
    loopbackListening("::1", port, 6),
  ]);
  return !ipv4 && !ipv6;
}

export async function findAvailablePort(start, end, portCount = 1, probe = portAvailable) {
  for (let port = start; port <= end - portCount + 1; port += 1) {
    const available = await Promise.all(
      Array.from({ length: portCount }, (_, offset) => probe(port + offset)),
    );
    if (available.every(Boolean)) return port;
  }
  return null;
}

export function configForDevPort(port, isolatedProfile = false) {
  return {
    ...(isolatedProfile ? { identifier: `com.eduardo.garimpolivre.dev${port}` } : {}),
    build: { devUrl: `http://localhost:${port}` },
  };
}
