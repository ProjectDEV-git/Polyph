import { meta, render } from "../src/site/routes";

export const metadata = meta("home", "en");

export default function Page() {
  return render("home", "en");
}
