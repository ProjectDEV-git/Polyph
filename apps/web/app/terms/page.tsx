import { meta, render } from "../../src/site/routes";

export const metadata = meta("terms", "en");

export default function Page() {
  return render("terms", "en");
}
