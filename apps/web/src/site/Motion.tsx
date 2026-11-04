"use client";
// The website's motion layer (marketing pages only): Lenis smooth scrolling,
// staggered entrances for each section's parts, and the marquee. Content is
// visible without script; under reduced motion Lenis stays off and
// entrances become short fades.
import { useEffect } from "react";

export function Motion() {
  useEffect(() => {
    const root = document.documentElement;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    root.classList.add("js");
    const items = Array.from(document.querySelectorAll<HTMLElement>(".site .reveal"));
    // Parts of one section enter one after another, 70ms apart.
    const groups = new Map<Element, number>();
    for (const el of items) {
      const parent = el.parentElement ?? el;
      const i = groups.get(parent) ?? 0;
      groups.set(parent, i + 1);
      el.style.transitionDelay = reduce ? "0ms" : `${Math.min(i, 6) * 70}ms`;
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            e.target.classList.add("in");
            io.unobserve(e.target);
          }
        }
      },
      { rootMargin: "0px 0px -8% 0px" },
    );
    items.forEach((el) => io.observe(el));

    let stop = () => {};
    if (!reduce) {
      let cancelled = false;
      void Promise.all([import("lenis"), import("gsap"), import("gsap/ScrollTrigger")]).then(
        ([{ default: Lenis }, { gsap }, { ScrollTrigger }]) => {
          if (cancelled) return;
          gsap.registerPlugin(ScrollTrigger);
          const lenis = new Lenis({ lerp: 0.12 });
          lenis.on("scroll", ScrollTrigger.update);
          const tick = (t: number) => lenis.raf(t * 1000);
          gsap.ticker.add(tick);
          gsap.ticker.lagSmoothing(0);
          // The marquee: one line of large type, linear, paused on hover.
          const tweens: gsap.core.Tween[] = [];
          document.querySelectorAll<HTMLElement>(".marquee .track").forEach((track) => {
            const tw = gsap.to(track, { xPercent: -50, duration: 38, ease: "none", repeat: -1 });
            tweens.push(tw);
            track.parentElement?.addEventListener("mouseenter", () => tw.pause());
            track.parentElement?.addEventListener("mouseleave", () => tw.play());
          });
          // A slow drift on the hero's room as the page scrolls.
          document.querySelectorAll<HTMLElement>("[data-drift]").forEach((el) => {
            gsap.to(el, {
              yPercent: -6,
              ease: "none",
              scrollTrigger: { trigger: el, start: "top top", end: "bottom top", scrub: true },
            });
          });
          stop = () => {
            gsap.ticker.remove(tick);
            tweens.forEach((t) => t.kill());
            ScrollTrigger.getAll().forEach((t) => t.kill());
            lenis.destroy();
          };
        },
      );
      const prev = stop;
      stop = () => {
        cancelled = true;
        prev();
      };
    }
    return () => {
      io.disconnect();
      stop();
    };
  }, []);
  return null;
}
