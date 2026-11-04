import { meta, render } from "../../src/site/routes";

export const metadata = meta("privacy", "en");

export default function Page() {
  return render("privacy", "en");
}
