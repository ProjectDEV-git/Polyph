import { meta, render } from "../../src/site/routes";

export const metadata = meta("pricing", "en");

export default function Page() {
  return render("pricing", "en");
}
