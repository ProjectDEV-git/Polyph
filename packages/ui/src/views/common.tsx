import { Button } from "../components/ui";
import { useApp } from "../ctx";
import { useT } from "../i18n";

/** An error inside the block that failed, with Retry (never full-screen). */
export function LoadError({ part }: { part: number }) {
  const l = useT();
  const app = useApp();
  return (
    <div role="alert" className="flex items-center gap-3 text-sm text-ink-2">
      <span>{l.t("app-load-failed")}</span>
      <Button variant="ghost" size="sm" onClick={() => void app.refresh(part)}>
        {l.t("app-retry")}
      </Button>
    </div>
  );
}
