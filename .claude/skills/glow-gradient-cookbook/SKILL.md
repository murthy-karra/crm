---
name: glow-gradient-cookbook
description: CSS recipes for a premium violet-gradient marketing-site look — diagonal hero gradients, layered glow-shadow buttons, IntersectionObserver scroll-reveal fade-ups, a seamless marquee, and a fill-bar animation. Reverse-engineered from litellm.ai/ai-gateway (a Webflow site, no JS animation library). Use when building a landing page, hero section, pricing page, or any marketing-style UI that wants gradients, glowing buttons, scroll-triggered reveals, or a logo marquee — or when the user references "that violet glow style" / "the litellm look" / "glow and rise".
---

# Glow & Gradient Cookbook

Working demo with live HTML/CSS/JS for every recipe: `references/cookbook.html` — read it directly for copy-pasteable code (the raw snippets also live in its `SNIPPETS` JS object).

Published artifact (same content, viewable in a browser): https://claude.ai/code/artifact/91a86bc3-fb08-4848-a616-caf98bee3fb4

Source investigated: https://www.litellm.ai/ai-gateway, 2026-09-01. Built on Webflow + jQuery — no GSAP/Framer/Lottie. All motion is plain CSS `@keyframes` plus one `IntersectionObserver`.

## Palette

| Token | Value | Use |
|---|---|---|
| violet | `#443afd` | primary accent, gradient start |
| violet-deep | `#5b3fd1` | button fills, glow ring |
| lavender | `#c3c0ff` | gradient end, highlight text |
| ink | `#0c0a32` | dark section background |
| paper | `#f3f3f3` / `#f7f7f9` | light section / card background |

Font: [Geist](https://fonts.google.com/specimen/Geist) for display/body, Geist Mono for code/labels — both on Google Fonts.

## Techniques

**1. Diagonal hero gradient**
```css
background: linear-gradient(170deg, #443afd 29%, #c3c0ff 121%);
```

**2. Dark-panel vignette** (fades a dark section to transparent)
```css
background: linear-gradient(
  #0c0a32 0%, rgba(12,10,50,.94) 26%,
  rgba(12,10,50,.55) 62%, rgba(12,10,50,.12) 88%, transparent 100%
);
```

**3. Layered glow-shadow button** — the signature effect: inset white top-highlight, a solid color ring, then two soft colored blooms.
```css
box-shadow:
  inset 0 1px 0 rgba(255,255,255,.6),
  0 0 0 4px #5b3fd1,
  0 0 16px 2px rgba(68,58,253,.45),
  0 0 34px 6px rgba(68,58,253,.22);
```

**4. Scroll reveal** — fade up once, on the way into view.
```css
@keyframes rise {
  from { opacity: 0; transform: translateY(.75rem); }
  to   { opacity: 1; transform: none; }
}
.reveal { opacity: 0; }
.reveal.in-view { animation: rise .7s cubic-bezier(.22,1,.36,1) forwards; }
```
```js
const io = new IntersectionObserver((entries) => {
  entries.forEach((entry, i) => {
    if (entry.isIntersecting) {
      entry.target.style.animationDelay = i * 90 + 'ms';
      entry.target.classList.add('in-view');
      io.unobserve(entry.target);
    }
  });
}, { threshold: .3 });
document.querySelectorAll('.reveal').forEach(el => io.observe(el));
```

**5. Marquee** — duplicate the row's children once, loop `translateX` linearly; the seam is invisible because the second half matches the first.
```css
@keyframes scroll {
  from { transform: translateX(0); }
  to   { transform: translateX(-50%); }
}
.marquee-track { display: flex; width: max-content; animation: scroll 26s linear infinite; }
```

**6. Fill bar** — same observer as scroll reveal, animates `width` instead of `opacity`.
```css
@keyframes fill {
  from { width: 0%; }
  to   { width: var(--fill-to, 92%); }
}
.fill-bar.in-view { animation: fill 1.8s cubic-bezier(.22,1,.36,1) forwards; }
```

## Notes

- All of this is achievable with plain CSS + one small observer — no animation library needed.
- Respect `prefers-reduced-motion: reduce` — collapse animation/transition durations to near-zero.
- The glow-shadow recipe is the most distinctive piece; reuse it for any CTA button, badge, or highlighted card to carry the identity across a page.
