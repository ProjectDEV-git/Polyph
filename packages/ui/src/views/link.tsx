// Link your phone (or another computer) to you in this room: a one-time
// code, shown as a QR code that opens the phone app.
import QRCode from "qrcode";
import { useEffect, useState } from "react";
import { Button } from "../components/ui";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";

type Link = { code: string; member: string; url: string; phone: boolean; expires_at: number };

export function LinkDevice() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const [link, setLink] = useState<Link | null>(null);
  const [svg, setSvg] = useState("");
  useEffect(() => {
    if (!link?.phone) return setSvg("");
    // Ink on the page ground: the code stays readable in both themes.
    QRCode.toString(link.url, {
      type: "svg",
      margin: 1,
      errorCorrectionLevel: "M",
      color: { dark: "#15161bff", light: "#ffffffff" },
    })
      .then(setSvg)
      .catch(() => setSvg(""));
  }, [link]);
  return (
    <div className="flex flex-col gap-3">
      <div>
        <Button
          onClick={async () => {
            const r = await app.act<Link>("room.link", { room });
            if (r) setLink(r);
          }}
        >
          {link ? l.t("app-link-again") : l.t("app-link-device")}
        </Button>
      </div>
      {link && (
        <div className="flex flex-wrap items-start gap-4" data-testid="link">
          {svg && (
            <div
              role="img"
              aria-label={l.t("app-link-qr")}
              className="size-44 shrink-0 overflow-hidden rounded-card bg-white p-1"
              // The SVG comes from the QR encoder, not from the network.
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          )}
          <div className="flex min-w-0 flex-1 flex-col gap-2 text-sm">
            <p className="text-ink-2">
              {link.phone ? l.t("app-link-scan") : l.t("app-link-no-phone")}
            </p>
            <p>
              <span className="text-ink-2">{l.t("app-link-code")} </span>
              <span className="font-mono text-base tracking-wide text-ink" data-testid="link-code">
                {link.code}
              </span>
            </p>
            <p className="break-all font-mono text-xs text-ink-2">
              {l.t("app-link-cli", { code: link.code, member: link.member })}
            </p>
            {link.phone && (
              <a
                className="break-all text-xs text-accent underline-offset-2 hover:underline"
                href={link.url}
                data-testid="link-url"
              >
                {l.t("app-link-open")}
              </a>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
