import { meta, render } from "../../src/site/routes";

export const metadata = meta("account", "en");

export default function Page() {
  return render("account", "en");
}
