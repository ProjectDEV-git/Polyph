import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Transport } from "../src/api/client";
import { PlatformCtx, StoreCtx } from "../src/ctx";
import { I18nProvider, Localizer } from "../src/i18n";
import { Store } from "../src/store";
import { Checklist, steps, type SetupStatus } from "../src/views/checklist";

const base: SetupStatus = {
  name: "Ann",
  rooms: 1,
  folders: [{ room: "r1", path: "/home/ann/pay" }],
  clients: [
    {
      id: "claude-code",
      name: "Claude Code",
      installed: true,
      connected: false,
      via_extension: false,
    },
    { id: "zed", name: "Zed", installed: false, connected: false, via_extension: false },
  ],
  agents: 0,
  members: 1,
};

function mount(status: SetupStatus, kind: "desktop" | "web" = "desktop") {
  const calls: [string, Record<string, unknown>][] = [];
  const api: Transport = {
    call: async <T,>(m: string, p: Record<string, unknown> = {}) => {
      calls.push([m, p]);
      if (m === "setup.status") return status as T;
      if (m === "clients.plan")
        return [
          {
            client: "claude-code",
            name: "Claude Code",
            file: "/home/ann/.claude.json",
            before: null,
            after: '{\n  "command": "/usr/bin/polyph"\n}',
            drops_comments: false,
          },
        ] as T;
      return {} as T;
    },
  };
  const store = new Store(api);
  store.set({ conn: "open", room: "r1" });
  render(
    <PlatformCtx.Provider value={{ kind }}>
      <StoreCtx.Provider value={store}>
        <I18nProvider l={new Localizer("en")}>
          <Checklist />
        </I18nProvider>
      </StoreCtx.Provider>
    </PlatformCtx.Provider>,
  );
  return calls;
}

describe("first-run checklist", () => {
  beforeEach(() => localStorage.clear());

  it("knows which steps are done", () => {
    expect(steps(base).map((s) => s.done)).toEqual([true, true, true, false, false, false]);
  });

  it("shows the next step and connects agents after showing the changes", async () => {
    const calls = mount(base);
    await screen.findByText("Getting started");
    expect(screen.getByText("3 of 6 done")).toBeTruthy();
    expect(screen.getByText("Found: Claude Code.")).toBeTruthy();
    fireEvent.click(screen.getByText("Review changes"));
    await screen.findByText("Claude Code: /home/ann/.claude.json");
    expect(document.querySelector("pre")!.textContent).toContain(
      '+   "command": "/usr/bin/polyph"',
    );
    fireEvent.click(screen.getByText("Write changes"));
    await waitFor(() => expect(calls.some(([m]) => m === "clients.connect")).toBe(true));
  });

  it("stays out of the way when done, hidden, or not on the desktop", async () => {
    const all: SetupStatus = {
      ...base,
      clients: [{ ...base.clients[0]!, connected: true }],
      agents: 1,
      members: 2,
    };
    const calls = mount(all);
    await waitFor(() => expect(calls.length).toBeGreaterThan(0));
    expect(screen.queryByTestId("checklist")).toBeNull();
    mount(base, "web");
    await waitFor(() => expect(screen.queryByTestId("checklist")).toBeNull());
  });

  it("can be hidden", async () => {
    mount(base);
    fireEvent.click(await screen.findByText("Hide"));
    await waitFor(() => expect(screen.queryByTestId("checklist")).toBeNull());
  });
});
