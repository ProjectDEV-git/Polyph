"use client";
// A plan's price from the account service. Prices are undecided, so this
// shows "Pricing coming soon" until the service reports one.
import { useEffect, useState } from "react";
import { CLOUD_API } from "./i18n";

let plans: Promise<Record<string, string | null>> | null = null;
function load(): Promise<Record<string, string | null>> {
  plans ??= fetch(`${CLOUD_API}/v1/plans`)
    .then((r) => (r.ok ? r.json() : { plans: [] }))
    .then((v: { plans?: { id: string; price: string | null }[] }) =>
      Object.fromEntries((v.plans ?? []).map((p) => [p.id, p.price])),
    )
    .catch(() => ({}));
  return plans;
}

export function PlanPrice({ plan, soon }: { plan: string; soon: string }) {
  const [price, setPrice] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void load().then((p) => {
      if (live) setPrice(p[plan] ?? null);
    });
    return () => {
      live = false;
    };
  }, [plan]);
  return <p className="price">{price ?? soon}</p>;
}
