import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";

// Não inicia Tauri nem usa a área de trabalho. Só fixtures, banco temporário e jsdom.
const localCargo=resolve(homedir(),".cargo/bin",process.platform==="win32"?"cargo.exe":"cargo");
const cargo=existsSync(localCargo)?localCargo:"cargo";
const cargoTarget=process.env.CARGO_TARGET_DIR??resolve("src-tauri","target-codex-offline");
const environment={...process.env,CARGO_TARGET_DIR:cargoTarget};
for(const [cmd,args,cwd] of [
  [cargo,["test","--quiet","headless_sete_lojas"],resolve("src-tauri")],
  [process.execPath,["--test","tests/multi-ui.test.js"],process.cwd()],
]) {
  const r=spawnSync(cmd,args,{cwd,stdio:"inherit",windowsHide:true,env:environment});
  if(r.error){console.error(r.error.message);process.exit(1);}
  if(r.status!==0)process.exit(r.status??1);
}
