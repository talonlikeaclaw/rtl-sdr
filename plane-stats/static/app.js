/* Chart interaction for /today's hour histogram and /activity's weekday×hour
   heatmap. Both are grids of small cells whose value only exists in a title
   attribute: no hover on touch, no title on focus, and 24 or 168 tab stops if
   every cell is reachable. This gives each chart one tab stop, arrow-key
   movement between cells, and one shared tooltip that follows hover, focus and
   tap. The cells keep tabindex="0" in the markup, so the charts still work if
   this file fails to load. */
(() => {
  "use strict";

  const tip = document.getElementById("chart-tip");
  const hide = () => {
    if (tip) tip.classList.remove("show");
  };

  const show = (cell) => {
    if (!tip) return;
    const text = cell.dataset.tip;
    if (!text) return;
    tip.textContent = text;
    tip.classList.add("show");
    const r = cell.getBoundingClientRect();
    const half = tip.offsetWidth / 2;
    const x = Math.min(
      Math.max(r.left + r.width / 2, half + 8),
      window.innerWidth - half - 8,
    );
    // Anchor to the bar top when the cell has one, so the tip clears the fill.
    const bar = cell.querySelector(".hist-bar");
    const anchor = bar ? bar.getBoundingClientRect().top : r.top;
    // transform is translate(-50%, -100%), so `top` is the tip's bottom edge.
    // On the first row that would sit over the hour axis, so flip below.
    const h = tip.offsetHeight;
    tip.style.top = anchor - 6 - h < 8 ? `${r.bottom + 6 + h}px` : `${anchor - 6}px`;
    tip.style.left = `${x}px`;
  };

  const cellOf = (node) => (node instanceof Element ? node.closest("[data-tip]") : null);

  const init = (root, cols) => {
    const cells = [...root.querySelectorAll("[data-tip]")];
    if (!cells.length) return;
    cells.forEach((c, i) => {
      c.tabIndex = i === 0 ? 0 : -1;
    });

    const go = (from, to) => {
      cells[from].tabIndex = -1;
      cells[to].tabIndex = 0;
      cells[to].focus();
      show(cells[to]);
    };

    root.addEventListener("keydown", (e) => {
      const cell = cellOf(e.target);
      if (!cell || !root.contains(cell)) return;
      const i = cells.indexOf(cell);
      if (i < 0) return;
      const rowStart = i - (i % cols);
      let next;
      switch (e.key) {
        case "ArrowLeft": next = i - 1; break;
        case "ArrowRight": next = i + 1; break;
        case "ArrowUp": next = i - cols; break;
        case "ArrowDown": next = i + cols; break;
        case "Home": next = rowStart; break;
        case "End": next = rowStart + cols - 1; break;
        case "Escape": hide(); return;
        default: return;
      }
      if (next < 0 || next >= cells.length || next === i) return;
      e.preventDefault();
      go(i, next);
    });

    root.addEventListener("pointerover", (e) => {
      if (e.pointerType !== "mouse") return;
      const cell = cellOf(e.target);
      if (cell) show(cell);
    });
    root.addEventListener("pointerout", (e) => {
      if (e.pointerType !== "mouse") return;
      const cell = cellOf(e.target);
      if (cell && !cell.contains(e.relatedTarget)) hide();
    });
    // Touch never hovers: a tap is the only path to the value.
    root.addEventListener("pointerdown", (e) => {
      const cell = cellOf(e.target);
      if (cell) show(cell);
    });
    root.addEventListener("focusin", (e) => {
      const cell = cellOf(e.target);
      if (cell) show(cell);
    });
    root.addEventListener("focusout", hide);
  };

  const hist = document.querySelector(".hist");
  if (hist) init(hist, 24);
  const heat = document.querySelector(".heatmap");
  if (heat) init(heat, 24);

  document.addEventListener("pointerdown", (e) => {
    if (!cellOf(e.target)) hide();
  });
  window.addEventListener("scroll", hide, { passive: true });
  window.addEventListener("resize", hide);
})();

/* The phone section menu is a native <details>, so it opens and closes
   without this file. Add the two dismissals a menu is expected to have:
   Escape returns focus to the button, and a tap outside closes it. */
(() => {
  const menu = document.querySelector(".nav-disclosure");
  if (!menu) return;
  const toggle = menu.querySelector("summary");
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !menu.open) return;
    menu.open = false;
    toggle.focus();
  });
  document.addEventListener("pointerdown", (e) => {
    if (menu.open && !menu.contains(e.target)) menu.open = false;
  });
  // A link that opens a new tab leaves the page in place; close the sheet so
  // it is not still covering the current page on return.
  menu.addEventListener("click", (e) => {
    if (e.target.closest("a")) menu.open = false;
  });
})();

/* Every htmx surface, one failure notice. htmx leaves a failed swap in place
   and says nothing, so a 500 or a dropped connection used to look exactly like
   a receiver that had gone quiet. The dashboard had this inline; /coverage has
   an auto-refreshing panel too and froze silently. Both now include
   partials/net-note.html and get the listeners here. */
(() => {
  const note = document.getElementById("net-note");
  if (!note) return;
  const show = (text) => {
    note.textContent = text;
    note.classList.add("show");
  };
  const hide = () => note.classList.remove("show");
  document.body.addEventListener("htmx:responseError", (e) => {
    show(`Update failed (${e.detail.xhr.status}). These panels are showing their last good data.`);
  });
  document.body.addEventListener("htmx:sendError", () => {
    show("Cannot reach the server. These panels are showing their last good data.");
  });
  document.body.addEventListener("htmx:timeout", () => {
    show("The server took too long to answer. These panels are showing their last good data.");
  });
  document.body.addEventListener("htmx:afterRequest", (e) => {
    if (e.detail.successful) hide();
  });
})();
