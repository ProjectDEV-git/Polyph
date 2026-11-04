// Just enough of the VS Code API for the modules under test.
export const Uri = {
  joinPath: (base: { path: string }, ...parts: string[]) => ({
    path: [base.path, ...parts].join("/"),
  }),
};
