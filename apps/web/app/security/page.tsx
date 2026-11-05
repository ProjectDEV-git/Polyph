import { meta, render } from "../../src/site/routes";

export const metadata = meta("security", "en");

export default function Page() {
  return render("security", "en");
}
