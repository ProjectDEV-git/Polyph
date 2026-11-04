// Runs inside VS Code (see run.mjs): the extension must connect to the real
// daemon, report its room and open the app panel without errors.
const vscode = require("vscode");

async function until(what, f, ms = 20000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await f()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`timed out: ${what}`);
}

exports.run = async () => {
  const ext = vscode.extensions.getExtension("polyph.polyph-vscode");
  if (!ext) throw new Error("extension not found");
  const api = await ext.activate();
  await until("connected", () => api.state() === "open");
  await until("room in status", () => api.status()?.room_name === process.env.E2E_ROOM);
  const cmds = await vscode.commands.getCommands(true);
  for (const c of ["polyph.openApp", "polyph.approvals", "polyph.reconnect", "polyph.pairPhone"])
    if (!cmds.includes(c)) throw new Error(`missing command ${c}`);
  await vscode.commands.executeCommand("polyph.openApp");
  await until("panel", () =>
    vscode.window.tabGroups.all.some((g) => g.tabs.some((t) => t.label === "Polyph")),
  );
};
