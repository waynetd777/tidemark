// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Tidemark's product page: the live hero chart, scroll reveals, the scrubbable week,
// the fill projection and the Ask chat. All the figures are made up, like the app's demo.
(() => {
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  // A repeatable random, so the made-up week looks the same on every visit.
  let seed = 7;
  const rand = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;

  // Run fn the first time el comes into view.
  const onceInView = (el, fn, threshold = 0.35) => {
    if (!el) return;
    const o = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          o.disconnect();
          fn();
        }
      },
      { threshold },
    );
    o.observe(el);
  };

  // Reveals.
  const reveals = new IntersectionObserver(
    (entries) => {
      for (const e of entries)
        if (e.isIntersecting) {
          e.target.classList.add("in");
          reveals.unobserve(e.target);
        }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );
  $$(".reveal").forEach((el) => reveals.observe(el));

  // The nav turns to glass once past the hero.
  const nav = $(".nav");
  const hero = $(".hero");

  // Stars over the hero sea, twinkling.
  const stars = $(".stars");
  const sctx = stars.getContext("2d");
  let dots = [];
  const sizeStars = () => {
    const dpr = devicePixelRatio || 1;
    stars.width = stars.clientWidth * dpr;
    stars.height = stars.clientHeight * dpr;
    sctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const n = Math.round((stars.clientWidth * stars.clientHeight) / 9000);
    dots = Array.from({ length: n }, () => ({
      x: Math.random() * stars.clientWidth,
      y: Math.random() * stars.clientHeight * 0.65,
      r: Math.random() * 1.1 + 0.2,
      p: Math.random() * Math.PI * 2,
    }));
  };
  const drawStars = (t) => {
    sctx.clearRect(0, 0, stars.clientWidth, stars.clientHeight);
    for (const d of dots) {
      const a = 0.25 + 0.45 * (0.5 + 0.5 * Math.sin(t / 1400 + d.p));
      sctx.globalAlpha = a * (1 - d.y / (stars.clientHeight * 0.7));
      sctx.fillStyle = "#dff4f8";
      sctx.beginPath();
      sctx.arc(d.x, d.y, d.r, 0, Math.PI * 2);
      sctx.fill();
    }
  };

  // The live chart: readings stream in, the peak band follows, and the dashed tidemark stays
  // at the highest reading on screen, with its label riding along.
  const live = $(".live");
  const lsvg = $(".live-chart", live);
  const W = 600,
    H = 160,
    N = 90;
  const lval = $("[data-live-val]");
  const ltm = $("[data-live-tm]");
  let cpu = 14;
  const nextCpu = () => {
    // Mostly calm, with the occasional burst: a build, an export.
    const burst = rand() < 0.035 ? 35 + rand() * 50 : 0;
    cpu = clamp(cpu * 0.82 + (8 + rand() * 12) * 0.18 + burst * 0.6, 2, 99);
    return cpu;
  };
  const series = Array.from({ length: N }, nextCpu);
  series[60] = 64;
  const peaks = series.map((v) => Math.min(99, v + 3 + rand() * 8));
  const yOf = (v) => H - (v / 100) * (H - 8);
  const pathOf = (vals) =>
    vals
      .map((v, i) => `${i ? "L" : "M"}${((i / (N - 1)) * W).toFixed(1)} ${yOf(v).toFixed(1)}`)
      .join("");
  let lastTm = 0;
  const drawLive = () => {
    const line = pathOf(series);
    $(".line", lsvg).setAttribute("d", line);
    $(".area", lsvg).setAttribute("d", `${line}L${W} ${H}L0 ${H}Z`);
    const top = pathOf(peaks);
    const bottom = series
      .map((v, i) => `L${((i / (N - 1)) * W).toFixed(1)} ${yOf(v).toFixed(1)}`)
      .reverse()
      .join("");
    $(".band", lsvg).setAttribute("d", `${top}${bottom}Z`);
    const tm = Math.max(...peaks);
    const y = yOf(tm);
    const l = $(".tm", lsvg);
    l.setAttribute("y1", y);
    l.setAttribute("y2", y);
    ltm.style.top = `${lsvg.offsetTop + (y / H) * lsvg.clientHeight}px`;
    ltm.textContent = `Tidemark ${Math.round(tm)}%`;
    if (Math.round(tm) > lastTm && lastTm) {
      ltm.classList.remove("new");
      void ltm.offsetWidth;
      ltm.classList.add("new");
    }
    lastTm = Math.round(tm);
    lval.textContent = Math.round(series[N - 1]);
  };
  drawLive();

  // Words that light up.
  const words = [];
  $$(".words").forEach((p) => {
    const split = (node) => {
      for (const child of [...node.childNodes]) {
        if (child.nodeType === Node.TEXT_NODE) {
          const frag = document.createDocumentFragment();
          for (const part of child.textContent.split(/(\s+)/)) {
            if (!part) continue;
            if (/^\s+$/.test(part)) {
              frag.append(part);
              continue;
            }
            const w = document.createElement("span");
            w.className = "w";
            w.textContent = part;
            words.push(w);
            frag.append(w);
          }
          child.replaceWith(frag);
        } else split(child);
      }
    };
    split(p);
  });

  // Now: eight gauges, each with a sparkline that keeps moving.
  const gauges = {
    cpu: { v: 7, lo: 3, hi: 30, fmt: (v) => Math.round(v), hot: 80 },
    gpu: { v: 11, lo: 2, hi: 40, fmt: (v) => Math.round(v), hot: 85 },
    mem: { v: 14.1, lo: 13.4, hi: 15.2, fmt: (v) => v.toFixed(1), drift: 0.05 },
    io: { v: 3.4, lo: 0.4, hi: 9, fmt: (v) => v.toFixed(1) },
    down: { v: 1.4, lo: 0.1, hi: 6, fmt: (v) => v.toFixed(1) },
    up: { v: 282, lo: 40, hi: 700, fmt: (v) => Math.round(v) },
    watts: { v: 8.8, lo: 4, hi: 22, fmt: (v) => v.toFixed(1) },
    disk: { v: 165, lo: 164.6, hi: 165.4, fmt: (v) => Math.round(v), drift: 0.01 },
  };
  for (const [k, g] of Object.entries(gauges)) {
    g.el = $(`[data-g="${k}"]`);
    g.b = $("b", g.el);
    g.p = $("path", g.el);
    g.s = [g.v];
    for (let k = 0; k < 39; k++)
      g.s.unshift(clamp(g.s[0] + (rand() - 0.5) * (g.hi - g.lo) * 0.18, g.lo, g.hi));
    g.s[g.s.length - 1] = g.v;
  }
  const stepGauge = (g) => {
    const span = g.hi - g.lo;
    const last = g.s[g.s.length - 1];
    // A drifting walk, with now and then a burst that settles again.
    let v = last + (rand() - 0.5) * span * (g.drift ? 0.25 : 0.16);
    if (!g.drift && rand() < 0.05) v += span * 0.45;
    if (!g.drift) v += (g.v - v) * 0.12;
    v = clamp(v, g.lo, g.hi);
    g.s.push(v);
    g.s.shift();
  };
  const drawGauge = (g) => {
    const min = Math.min(...g.s),
      max = Math.max(...g.s);
    const r = max - min || 1;
    g.p.setAttribute(
      "d",
      g.s
        .map(
          (v, i) =>
            `${i ? "L" : "M"}${((i / (g.s.length - 1)) * 120).toFixed(1)} ${(34 - ((v - min) / r) * 30).toFixed(1)}`,
        )
        .join(""),
    );
    g.b.textContent = g.fmt(g.s[g.s.length - 1]);
  };
  Object.values(gauges).forEach(drawGauge);

  // Performance: a made-up working week, five days with the nights asleep.
  const scrub = $("[data-scrub]");
  const ssvg = $("svg", scrub);
  const SW = 1000,
    SH = 240,
    SN = 5 * 96;
  const apps = ["Xcode", "Safari", "Mail", "Photos", "Slack", "Music", "Finder", "Spotlight"];
  const week = [];
  for (let i = 0; i < SN; i++) {
    const hour = (i % 96) / 4;
    const asleep = hour < 7 || hour > 23;
    let v = asleep ? null : 6 + rand() * 10 + (hour > 9 && hour < 18 ? 8 + rand() * 12 : 0);
    if (v !== null && rand() < 0.02) v += 30 + rand() * 30;
    week.push(v);
  }
  // Wednesday's build, the week's high.
  const PEAK = 2 * 96 + 60;
  for (let j = -6; j <= 6; j++) week[PEAK + j] = 96 - Math.abs(j) * (8 + rand() * 4);
  const syOf = (v) => SH - (v / 100) * (SH - 10);
  const sx = (i) => (i / (SN - 1)) * SW;
  const runs = [];
  let run = null;
  week.forEach((v, i) => {
    if (v === null) run = null;
    else {
      if (!run) runs.push((run = []));
      run.push(i);
    }
  });
  const runPath = (r, f) =>
    r.map((i, k) => `${k ? "L" : "M"}${sx(i).toFixed(1)} ${syOf(f(i)).toFixed(1)}`).join("");
  const peakOf = (i) => Math.min(100, week[i] + 4 + ((i * 37) % 9));
  $(".line", ssvg).setAttribute("d", runs.map((r) => runPath(r, (i) => week[i])).join(""));
  $(".area", ssvg).setAttribute(
    "d",
    runs
      .map((r) => `${runPath(r, (i) => week[i])}L${sx(r[r.length - 1])} ${SH}L${sx(r[0])} ${SH}Z`)
      .join(""),
  );
  $(".band", ssvg).setAttribute(
    "d",
    runs
      .map((r) => {
        const top = runPath(r, peakOf);
        const back = [...r]
          .reverse()
          .map((i) => `L${sx(i).toFixed(1)} ${syOf(week[i]).toFixed(1)}`)
          .join("");
        return `${top}${back}Z`;
      })
      .join(""),
  );
  // Nights asleep, shaded.
  const sleepsG = $(".sleeps", ssvg);
  let gapStart = null;
  week.forEach((v, i) => {
    if (v === null && gapStart === null) gapStart = i;
    if ((v !== null || i === SN - 1) && gapStart !== null) {
      const r = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      r.setAttribute("x", sx(gapStart));
      r.setAttribute("width", sx(i) - sx(gapStart));
      r.setAttribute("y", 0);
      r.setAttribute("height", SH);
      sleepsG.append(r);
      gapStart = null;
    }
  });
  const tmY = syOf(96);
  $(".tm", ssvg).setAttribute("y1", tmY);
  $(".tm", ssvg).setAttribute("y2", tmY);
  const days = ["Mon", "Tue", "Wed", "Thu", "Fri"];
  const when = (i) => {
    const m = (i % 96) * 15 + (i === PEAK ? 12 : 0);
    return `${days[Math.floor(i / 96)]} ${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  };
  const appsAt = (i, total) => {
    // The busiest apps at that moment: Xcode carries Wednesday's build.
    const near = Math.abs(i - PEAK) < 8;
    const pick = near
      ? [0, 1, 6]
      : [(i * 3) % apps.length, (i * 5 + 1) % apps.length, (i * 7 + 2) % apps.length];
    const uniq = [...new Set(pick)];
    const shares = near ? [0.74, 0.1, 0.05] : [0.42, 0.22, 0.12];
    return uniq.map((a, k) => ({ name: apps[a], pct: Math.max(1, Math.round(total * shares[k])) }));
  };
  const scCard = {
    when: $(".sc-when", scrub),
    big: $(".sc-big b", scrub),
    apps: $(".sc-apps", scrub),
  };
  scCard.apps.innerHTML = "<li><span></span><span></span><i></i></li>".repeat(3);
  const showAt = (i) => {
    i = clamp(Math.round(i), 0, SN - 1);
    // A sleeping moment shows the nearest waking one.
    let j = i;
    for (let d = 0; week[j] === null && d < SN; d++)
      j = week[i + d] != null ? i + d : week[i - d] != null ? i - d : j;
    const v = week[j];
    $(".cursor", ssvg).setAttribute("x1", sx(j));
    $(".cursor", ssvg).setAttribute("x2", sx(j));
    $(".dot", ssvg).setAttribute("cx", sx(j));
    $(".dot", ssvg).setAttribute("cy", syOf(v));
    scCard.when.textContent = when(j);
    scCard.big.textContent = `${Math.round(v)}%`;
    const list = appsAt(j, v);
    $$("li", scCard.apps).forEach((li, k) => {
      const a = list[k];
      li.style.visibility = a ? "" : "hidden";
      if (!a) return;
      li.children[0].textContent = a.name;
      li.children[1].textContent = `${a.pct}%`;
      li.children[2].style.setProperty("--w", `${a.pct}%`);
    });
  };
  showAt(PEAK);
  let scrubbed = false;
  const chartEl = $(".scrub-chart", scrub);
  const fromEvent = (e) => {
    const r = ssvg.getBoundingClientRect();
    return ((e.clientX - r.left) / r.width) * (SN - 1);
  };
  chartEl.addEventListener("pointermove", (e) => {
    scrubbed = true;
    showAt(fromEvent(e));
  });
  chartEl.addEventListener("pointerdown", (e) => {
    scrubbed = true;
    showAt(fromEvent(e));
  });
  chartEl.addEventListener("pointerleave", () => showAt(PEAK));

  // Trends: a year of disk space creeping up, and where it would meet the top.
  const fill = $(".fill-chart");
  const FW = 600,
    FH = 260,
    FULL = 494,
    LOW = 150;
  const fy = (gbs) => 30 + (1 - (gbs - LOW) / (FULL - LOW)) * (FH - 50);
  const year = [];
  let used = 210;
  for (let m = 0; m <= 52; m++) {
    used += 2.4 + rand() * 1.6 - (m === 30 ? 26 : 0);
    year.push(used);
  }
  const fx = (k) => (k / 52) * 400;
  // A week's growth over the last quarter, and the year shifted so the disk fills in about 20 weeks.
  const rate = (year[year.length - 1] - year[year.length - 13]) / 12;
  const shift = FULL - rate * 20 - year[year.length - 1];
  for (let k = 0; k < year.length; k++) year[k] += shift;
  const last = year[year.length - 1];
  const weeksToFull = (FULL - last) / rate;
  const yearLine = year
    .map((v, k) => `${k ? "L" : "M"}${fx(k).toFixed(1)} ${fy(v).toFixed(1)}`)
    .join("");
  $(".line", fill).setAttribute("d", yearLine);
  $(".area", fill).setAttribute("d", `${yearLine}L400 ${FH - 20}L0 ${FH - 20}Z`);
  const hitX = fx(52 + weeksToFull);
  $(".proj", fill).setAttribute("d", `M400 ${fy(last).toFixed(1)}L${hitX.toFixed(1)} ${fy(FULL)}`);
  const hit = $(".hit", fill);
  hit.setAttribute("cx", Math.min(hitX, FW - 8));
  hit.setAttribute("cy", fy(FULL));
  $("[data-fill-rate]").textContent = `+${Math.round(rate * 4.35)} GB a month`;
  const fullDate = new Date(Date.now() + weeksToFull * 7 * 86400000);
  $("[data-fill-when]").textContent = fullDate.toLocaleDateString("en-GB", {
    month: "long",
    year: "numeric",
  });
  const fline = $(".line", fill);
  const flen = fline.getTotalLength();
  fline.style.strokeDasharray = flen;
  fline.style.strokeDashoffset = still ? 0 : flen;
  onceInView(fill, () => {
    fline.style.transition = "stroke-dashoffset 1.6s cubic-bezier(.2,.7,.1,1)";
    fline.style.strokeDashoffset = 0;
  });

  // CPU by weekday and hour.
  const heat = $("[data-heat]");
  const wd = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  wd.forEach((d, di) => {
    const l = document.createElement("span");
    l.textContent = d;
    heat.append(l);
    for (let h = 0; h < 24; h++) {
      const c = document.createElement("i");
      const work = di < 5 && h >= 8 && h <= 18;
      const peak = di < 5 && h >= 14 && h <= 16;
      const o = h < 7 ? 0.06 : work ? 0.35 + rand() * 0.3 + (peak ? 0.3 : 0) : 0.1 + rand() * 0.18;
      c.style.setProperty("--o", Math.min(1, o).toFixed(2));
      c.style.transitionDelay = `${((di * 24 + h) * 0.006).toFixed(3)}s`;
      heat.append(c);
    }
  });

  // Ask: the answer types itself out.
  const typed = $("[data-typed]");
  const answer =
    "Wednesday at 14:57 the CPU reached 96%, the week's tidemark. For four minutes Xcode was using 71% of it, building, with Safari at 9%. Memory was fine. That build is what you felt.";
  const typeAnswer = async () => {
    for (;;) {
      typed.textContent = "";
      await sleep(900);
      for (const ch of answer) {
        typed.textContent += ch;
        await sleep(ch === "." || ch === "," ? 140 : 18 + Math.random() * 22);
      }
      await sleep(6000);
    }
  };

  if (still) {
    words.forEach((w) => (w.style.opacity = 1));
    typed.textContent = answer;
    $$(".tray-drop").forEach((t) => (t.style.cssText = "opacity:1;transform:none"));
    drawStars(0);
    return;
  }
  onceInView($("[data-chat]"), typeAnswer, 0.5);

  // Streams: the hero chart every 700 ms, the gauges every second, the week's cursor sweeping
  // until someone moves it, the stars every frame while the hero shows.
  let heroOn = true,
    gaugesOn = false,
    scrubOn = false;
  new IntersectionObserver(([e]) => (heroOn = e.isIntersecting)).observe(hero);
  new IntersectionObserver(([e]) => (gaugesOn = e.isIntersecting)).observe($("[data-gauges]"));
  new IntersectionObserver(([e]) => (scrubOn = e.isIntersecting)).observe(scrub);
  setInterval(() => {
    if (!heroOn) return;
    series.push(nextCpu());
    series.shift();
    const v = series[N - 1];
    peaks.push(Math.min(99, v + 2 + rand() * 9));
    peaks.shift();
    drawLive();
  }, 700);
  setInterval(() => {
    if (!gaugesOn) return;
    for (const g of Object.values(gauges)) {
      stepGauge(g);
      drawGauge(g);
    }
  }, 1000);
  let sweep = PEAK - 140;
  setInterval(() => {
    if (!scrubOn || scrubbed) return;
    sweep += 2;
    if (sweep > PEAK) {
      sweep = PEAK;
      showAt(PEAK);
      scrubbed = true;
      return;
    }
    showAt(sweep);
  }, 30);

  const starLoop = (t) => {
    if (heroOn) drawStars(t);
    requestAnimationFrame(starLoop);
  };
  sizeStars();
  requestAnimationFrame(starLoop);

  // Scroll-linked motion.
  const ebb = $("[data-ebb]");
  const statement = $(".statement");
  let ticking = false;
  const frame = () => {
    ticking = false;
    const vh = innerHeight;
    nav.classList.toggle("solid", hero.getBoundingClientRect().bottom < 60);

    // The Now screen tilts up out of the water and grows to full size.
    const r = ebb.getBoundingClientRect();
    const p = clamp(1 - (r.top - vh * 0.12) / (vh * 0.75));
    ebb.style.transform = `rotateX(${(1 - p) * 18}deg) scale(${0.86 + p * 0.14}) translateY(${(1 - p) * 30}px)`;

    const s = statement.getBoundingClientRect();
    const sp = clamp((vh * 0.85 - s.top) / (s.height + vh * 0.2));
    const lit = sp * words.length * 1.15;
    words.forEach((w, i) => (w.style.opacity = String(0.16 + 0.84 * clamp(lit - i))));
  };
  const onScroll = () => {
    if (!ticking) {
      ticking = true;
      requestAnimationFrame(frame);
    }
  };
  addEventListener("scroll", onScroll, { passive: true });
  let rt;
  addEventListener("resize", () => {
    onScroll();
    clearTimeout(rt);
    rt = setTimeout(() => {
      sizeStars();
      drawLive();
    }, 150);
  });
  frame();
})();
