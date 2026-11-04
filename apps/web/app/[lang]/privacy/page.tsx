import { langParams, meta, render } from "../../../src/site/routes";

export const dynamicParams = false;
export const generateStaticParams = langParams;

type Props = { params: Promise<{ lang: string }> };

export async function generateMetadata({ params }: Props) {
  return meta("privacy", (await params).lang);
}

export default async function Page({ params }: Props) {
  return render("privacy", (await params).lang);
}
