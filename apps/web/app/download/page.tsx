import { meta, render } from "../../src/site/routes";

export const metadata = meta("download", "en");

export default function Page() {
  return render("download", "en");
}
