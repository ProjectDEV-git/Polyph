// A small stand-in for the `vscode` module: records what the extension
// registers and shows, and lets tests answer prompts.
export const recorded = {
  commands: new Map<string, (...a: unknown[]) => unknown>(),
  decorationProviders: [] as {
    provideFileDecoration(u: Uri): any;
    onDidChangeFileDecorations: any;
  }[],
  warnings: [] as { text: string; items: string[] }[],
  infos: [] as string[],
  answer: undefined as string | undefined,
  input: undefined as string | undefined,
  clipboard: "",
  bar: null as StatusBarItem | null,
  config: {} as Record<string, string>,
};

export class Uri {
  constructor(
    readonly scheme: string,
    readonly fsPath: string,
  ) {}
  static file(p: string) {
    return new Uri("file", p);
  }
  static parse(s: string) {
    return new Uri("https", s);
  }
  static joinPath(u: Uri, ...p: string[]) {
    return new Uri(u.scheme, [u.fsPath, ...p].join("/"));
  }
  toString() {
    return `${this.scheme}://${this.fsPath}`;
  }
}

export class ThemeColor {
  constructor(readonly id: string) {}
}
export class Range {
  constructor(...a: number[]) {
    void a;
  }
}
export class EventEmitter<T> {
  private fs: ((v: T) => void)[] = [];
  event = (f: (v: T) => void) => {
    this.fs.push(f);
    return { dispose() {} };
  };
  fire(v: T) {
    for (const f of this.fs) f(v);
  }
  dispose() {}
}

export class StatusBarItem {
  text = "";
  tooltip: string | undefined;
  command: string | undefined;
  backgroundColor: unknown;
  show() {}
  dispose() {}
}

export const StatusBarAlignment = { Left: 1, Right: 2 };
export const OverviewRulerLane = { Left: 1 };
export const ViewColumn = { Active: -1 };
export const ColorThemeKind = { Light: 1, Dark: 2, HighContrast: 3, HighContrastLight: 4 };

const disposable = { dispose() {} };

export const window = {
  activeColorTheme: { kind: 2 },
  visibleTextEditors: [] as unknown[],
  createStatusBarItem() {
    recorded.bar = new StatusBarItem();
    return recorded.bar;
  },
  registerFileDecorationProvider(p: (typeof recorded.decorationProviders)[number]) {
    recorded.decorationProviders.push(p);
    return disposable;
  },
  createTextEditorDecorationType() {
    return disposable;
  },
  onDidChangeVisibleTextEditors() {
    return disposable;
  },
  registerWebviewViewProvider() {
    return disposable;
  },
  async showWarningMessage(text: string, ...rest: unknown[]) {
    const items = rest.filter((x): x is string => typeof x === "string");
    recorded.warnings.push({ text, items });
    return recorded.answer;
  },
  async showInformationMessage(text: string) {
    recorded.infos.push(text);
    return undefined;
  },
  async showErrorMessage(text: string) {
    recorded.infos.push(`error: ${text}`);
    return undefined;
  },
  async showInputBox() {
    return recorded.input;
  },
  async showQuickPick<T>(items: T[]) {
    return items[0];
  },
  createWebviewPanel() {
    throw new Error("not in tests");
  },
};

export const workspace = {
  workspaceFolders: [] as { uri: Uri }[],
  getConfiguration() {
    return { get: (k: string) => recorded.config[k] };
  },
  onDidChangeTextDocument() {
    return disposable;
  },
  onDidChangeConfiguration() {
    return disposable;
  },
};

export const commands = {
  registerCommand(id: string, f: (...a: unknown[]) => unknown) {
    recorded.commands.set(id, f);
    return disposable;
  },
  async executeCommand(id: string, ...a: unknown[]) {
    return recorded.commands.get(id)?.(...a);
  },
};

export const env = {
  language: "en",
  clipboard: {
    async writeText(s: string) {
      recorded.clipboard = s;
    },
  },
  async openExternal() {
    return true;
  },
};
