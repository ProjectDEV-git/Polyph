// Registers Polyph as an MCP server for Copilot and other agents in VS Code,
// so they join the room with Polyph's tools without any settings file.
// Copilot starts `polyph mcp --client vscode` in the first workspace folder,
// which the daemon maps to that folder's room.
//
// The provider API arrived in VS Code 1.101; older versions simply do not
// get it (the rest of the extension works the same). It is typed here so the
// extension builds against the oldest supported @types/vscode.
import * as vscode from "vscode";

interface ProviderApi {
  registerMcpServerDefinitionProvider(
    id: string,
    provider: {
      onDidChangeMcpServerDefinitions?: vscode.Event<void>;
      provideMcpServerDefinitions(): unknown[];
    },
  ): vscode.Disposable;
}

interface StdioDefinition {
  cwd?: vscode.Uri;
}

type StdioCtor = new (
  label: string,
  command: string,
  args?: string[],
  env?: Record<string, string | number | null>,
  version?: string,
) => StdioDefinition;

/** The provider id; it must match `mcpServerDefinitionProviders` in package.json. */
export const PROVIDER = "polyph";

/** The definition Copilot starts: `polyph mcp --client vscode` in `cwd`. */
export function definition(
  Stdio: StdioCtor,
  command: string,
  home: string | undefined,
  cwd: vscode.Uri | undefined,
  version: string,
): StdioDefinition {
  const env: Record<string, string> = home ? { POLYPH_HOME: home } : {};
  const d = new Stdio("Polyph", command, ["mcp", "--client", "vscode"], env, version);
  if (cwd) d.cwd = cwd;
  return d;
}

/** Registers the provider when this VS Code has the API; returns whether it did. */
export function registerMcp(
  ctx: vscode.ExtensionContext,
  command: () => string,
  home: () => string | undefined,
  version: string,
): boolean {
  const api = vscode as unknown as {
    lm?: Partial<ProviderApi>;
    McpStdioServerDefinition?: StdioCtor;
  };
  const Stdio = api.McpStdioServerDefinition;
  if (!api.lm?.registerMcpServerDefinitionProvider || !Stdio) return false;
  const changed = new vscode.EventEmitter<void>();
  ctx.subscriptions.push(
    changed,
    api.lm.registerMcpServerDefinitionProvider(PROVIDER, {
      onDidChangeMcpServerDefinitions: changed.event,
      provideMcpServerDefinitions: () => [
        definition(Stdio, command(), home(), vscode.workspace.workspaceFolders?.[0]?.uri, version),
      ],
    }),
    // A new command or home, or another folder, means a new definition.
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("polyph")) changed.fire();
    }),
  );
  const folders = (vscode.workspace as { onDidChangeWorkspaceFolders?: vscode.Event<unknown> })
    .onDidChangeWorkspaceFolders;
  if (folders) ctx.subscriptions.push(folders(() => changed.fire()));
  return true;
}
