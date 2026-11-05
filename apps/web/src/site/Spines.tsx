"use client";
// Surfaces as a deck of cards: scrolling slides the next card in and folds
// the earlier ones into spines at the left. On phones they simply stack.
import { useEffect, useRef, useState } from "react";

export type SurfaceCard = {
  key: string;
  title: string;
  text: string;
  where: string;
  link: string;
  linkLabel: string;
};

const SPINE = 120;
const GAP = 8;

export function Spines({
  title,
  count,
  cards,
}: {
  title: string;
  count: string;
  cards: SurfaceCard[];
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const phone = window.matchMedia("(max-width: 720px)");
    const onScroll = () => {
      if (phone.matches) return;
      const r = el.getBoundingClientRect();
      const span = r.height - window.innerHeight;
      const p = span > 0 ? Math.min(1, Math.max(0, -r.top / span)) : 0;
      setActive(Math.min(cards.length - 1, Math.floor(p * cards.length)));
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, [cards.length]);
  const show = (i: number) => {
    const el = wrap.current;
    if (!el) return setActive(i);
    const span = el.offsetHeight - window.innerHeight;
    const top = el.getBoundingClientRect().top + window.scrollY + (span * (i + 0.5)) / cards.length;
    window.scrollTo({
      top,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
    setActive(i);
  };
  const cardW = "min(620px, 70vw)";
  return (
    <section className="spines" ref={wrap} aria-labelledby="surfaces-title">
      <div className="stage">
        <h2 id="surfaces-title" className="reveal">
          {title}
        </h2>
        <div className="deck">
          {cards.map((c, i) => {
            const folded = i < active;
            const left = folded
              ? `${i * SPINE}px`
              : i === active
                ? `${active * SPINE}px`
                : `calc(${active * SPINE}px + ${i - active} * (${cardW} + ${GAP}px))`;
            return (
              <article
                key={c.key}
                className={`scard${folded ? " folded" : ""}`}
                style={{ left, width: folded ? `${SPINE - GAP}px` : cardW }}
                onClick={folded ? () => show(i) : undefined}
                aria-label={c.title}
              >
                <h3>
                  {folded ? (
                    <button
                      type="button"
                      onClick={() => show(i)}
                      style={{ all: "unset", cursor: "pointer" }}
                    >
                      {c.title}
                    </button>
                  ) : (
                    c.title
                  )}
                </h3>
                <p>{c.text}</p>
                <div className="foot">
                  <span className="soft">{c.where}</span>
                  <a href={c.link} tabIndex={folded ? -1 : undefined}>
                    {c.linkLabel}
                  </a>
                </div>
              </article>
            );
          })}
        </div>
        <p style={{ fontSize: 17 }}>{count}</p>
      </div>
    </section>
  );
}
