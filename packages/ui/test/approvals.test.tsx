import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Transport } from "../src/api/client";
import { PlatformCtx, StoreCtx } from "../src/ctx";
import { I18nProvider, Localizer } from "../src/i18n";
import { Store } from "../src/store";
import { ApprovalStack, describeApproval } from "../src/views/approvals";

const approval = {
  id: "p1",
  agent: { id: "a1", member: "m1", device: "d", kind: "claude", label: "Ann/claude" },
  kind: { t: "command", c: { command: "cargo publish", cwd: "/home/ann/code/payments" } },
  task: "card1",
  created_at: Date.now() / 1000,
  escalate_at: 0,
};

describe("approvals", () => {
  it("says exactly what will run, in every language", () => {
    for (const lang of ["en", "th", "ja", "zh-Hans"] as const) {
      const text = describeApproval(new Localizer(lang), approval);
      expect(text).toContain("cargo publish");
      expect(text).toContain("Ann/claude");
    }
  });

  it("approves for the task and denies with a note", async () => {
    const calls: [string, Record<string, unknown>][] = [];
    const api: Transport = {
      call: async <T,>(m: string, p: Record<string, unknown> = {}) => {
        calls.push([m, p]);
        return {} as T;
      },
    };
    const store = new Store(api);
    store.set({
      room: "r1",
      me: { name: "Ann", member: "m1", device: "d", relay: "", keystore: "" },
      approvals: [approval],
    });
    const l = new Localizer("en");
    render(
      <PlatformCtx.Provider value={{ kind: "web" }}>
        <StoreCtx.Provider value={store}>
          <I18nProvider l={l}>
            <ApprovalStack />
          </I18nProvider>
        </StoreCtx.Provider>
      </PlatformCtx.Provider>,
    );
    expect(screen.getByText("cargo publish")).toBeTruthy();
    fireEvent.click(screen.getByText("Approve for this task"));
    await waitFor(() =>
      expect(calls[0]).toEqual(["approve", { room: "r1", id: "p1", task: true }]),
    );
    fireEvent.click(screen.getByText("Deny"));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "not on main" } });
    fireEvent.click(screen.getByText("Deny with note"));
    await waitFor(() =>
      expect(calls[1]).toEqual(["deny", { room: "r1", id: "p1", note: "not on main" }]),
    );
  });
});
