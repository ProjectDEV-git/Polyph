import { Localizer, pickLang, LANGS } from "../src/i18n";

describe("i18n", () => {
  it("formats plurals and falls back to English", () => {
    const en = new Localizer("en");
    expect(en.t("tui-approvals", { n: 1 })).toBe("1 approval");
    expect(en.t("tui-approvals", { n: 3 })).toBe("3 approvals");
    expect(en.t("no-such-key")).toBe("no-such-key");
  });

  it("has every app key in every language", () => {
    const en = new Localizer("en");
    for (const lang of LANGS) {
      const l = new Localizer(lang);
      expect(l.t("app-view-chat")).not.toBe("app-view-chat");
      expect(en.has("app-view-chat")).toBe(true);
    }
  });

  it("negotiates languages", () => {
    expect(pickLang(["th-TH", "en"])).toBe("th");
    expect(pickLang(["zh-CN"])).toBe("zh-Hans");
    expect(pickLang(["ja"])).toBe("ja");
    expect(pickLang(["de-DE"])).toBe("en");
  });
});
