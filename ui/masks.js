// Shiki's masks, shared by the island and the Loadout window.
// Every mask is drawn in a 32×32 viewBox with the same moving parts:
//   .head  turns toward the cursor        .body  breathes
//   .eyes  blinks                         .iris  follows the cursor
//   .glow  takes the state colour (--eye); the aura uses --soul
(function () {
  let uid = 0;

  const MASKS = {
    hannya: {
      name: 'Hannya',
      desc: 'Topeng iblis wanita dari teater Noh. Tanduk emas, mata emas, senyum yang bikin merinding.',
      idle: { soul: '#ffb43a', eye: '#d9a33a' },
      gaze: [1.15, 0.55],
      svg: id => {
        const L = 'M9.3 15.7Q11.5 13.9 14 15.3Q13.5 18.1 11.4 18.3Q9.7 17.9 9.3 15.7Z';
        const R = 'M22.7 15.7Q20.5 13.9 18 15.3Q18.5 18.1 20.6 18.3Q22.3 17.9 22.7 15.7Z';
        return `<defs>
          <radialGradient id="${id}-skin" cx="50%" cy="42%" r="68%"><stop offset="0%" stop-color="#fdfcf8"/><stop offset="62%" stop-color="#ebe6db"/><stop offset="100%" stop-color="#a59d8c"/></radialGradient>
          <linearGradient id="${id}-gold" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#ffe7a3"/><stop offset="50%" stop-color="#c9952e"/><stop offset="100%" stop-color="#6e4b0e"/></linearGradient>
          <clipPath id="${id}-l"><path d="${L}"/></clipPath><clipPath id="${id}-r"><path d="${R}"/></clipPath>
        </defs>
        <g class="head"><g class="body">
          <path d="M9.8 10.8C6.4 10 3.2 7.2 2.9 2.2 4.6 5.4 7.8 6.8 11.6 7.5z" fill="url(#${id}-gold)"/>
          <path d="M22.2 10.8C25.6 10 28.8 7.2 29.1 2.2 27.4 5.4 24.2 6.8 20.4 7.5z" fill="url(#${id}-gold)"/>
          <path d="M7.2 12.6C7.4 7.8 11.3 4.8 16 4.8s8.6 3 8.8 7.8l-.5 3.4H7.7z" fill="#0e0c0b"/>
          <path d="M16 5.7c1.6 1.8 5.2 3.6 7.6 5.8 1.4 1.6 1.8 4 1.4 6.2-.4 2.8-1.4 5-2.6 7-1.2 2.4-3.4 4.8-6.4 5-3-.2-5.2-2.6-6.4-5-1.2-2-2.2-4.2-2.6-7-.4-2.2 0-4.6 1.4-6.2 2.4-2.2 6-4 7.6-5.8z" fill="url(#${id}-skin)"/>
          <path d="M15.1 5.5c-1.8 1.2-4.6 2.8-7 5M14.4 5c-2 1.1-4.8 2.6-7.2 4.7M16.9 5.5c1.8 1.2 4.6 2.8 7 5M17.6 5c2 1.1 4.8 2.6 7.2 4.7" stroke="#f4f0e6" stroke-width="0.42" fill="none" stroke-linecap="round"/>
          <path d="M16 6.4V8.1M16 8.3l.9 2-.9 1.8-.9-1.8z" stroke="#c4bcab" stroke-width="0.38" fill="none" stroke-linejoin="round"/>
          <path d="M15.3 13.7c-1.4-1-3.6-1-5.7 0M15.5 12.4c-1.4-1.2-3.4-1.2-5 -.4M16.7 13.7c1.4-1 3.6-1 5.7 0M16.5 12.4c1.4-1.2 3.4-1.2 5-.4M16 12.7v2.4" stroke="#8f8676" stroke-width="0.48" fill="none" stroke-linecap="round"/>
          <g class="eyes">
            <path class="glow" d="${L}" stroke="#0e0c0b" stroke-width="0.6"/>
            <path class="glow" d="${R}" stroke="#0e0c0b" stroke-width="0.6"/>
            <g clip-path="url(#${id}-l)"><circle class="iris" cx="11.7" cy="16.2" r="0.95" fill="#130b04"/></g>
            <g clip-path="url(#${id}-r)"><circle class="iris" cx="20.3" cy="16.2" r="0.95" fill="#130b04"/></g>
          </g>
          <path d="M15.1 15.6q-.6 2.5-1 4.5M16.9 15.6q.6 2.5 1 4.5" stroke="#b6ae9e" stroke-width="0.35" fill="none" stroke-linecap="round"/>
          <path d="M14.1 20.3q1.9 1.1 3.8 0" stroke="#8a8272" stroke-width="0.45" fill="none" stroke-linecap="round"/>
          <ellipse cx="14.9" cy="20.3" rx="0.45" ry="0.3" fill="#2a241e"/><ellipse cx="17.1" cy="20.3" rx="0.45" ry="0.3" fill="#2a241e"/>
          <path d="M12.3 21c-1.8 1-2.6 2.6-2.4 4.6M19.7 21c1.8 1 2.6 2.6 2.4 4.6" stroke="#a1998a" stroke-width="0.42" fill="none" stroke-linecap="round"/>
          <path d="M10.6 22.8q5.4 1.8 10.8 0 0 4-5.4 4.8-5.4-.8-5.4-4.8z" fill="#1b0f07" stroke="#4a3b2c" stroke-width="0.35"/>
          <path d="M11.6 23.4q4.4 1.4 8.8 0l-.2.9q-4.2 1.2-8.4 0z" fill="url(#${id}-gold)"/>
          <path d="M12.4 26.1q3.6 1.2 7.2 0l-.2-.6q-3.4 1.1-6.8 0z" fill="url(#${id}-gold)"/>
          <path d="M12 23.6l.9 2.4.6-1.8zM20 23.6l-.9 2.4-.6-1.8zM13 27.1l.5-1.9.6 1.8zM19 27.1l-.5-1.9-.6 1.8z" fill="url(#${id}-gold)"/>
          <path d="M13.6 28.6q2.4.7 4.8 0" stroke="#b3ab9b" stroke-width="0.4" fill="none" stroke-linecap="round"/>
        </g></g>`;
      },
    },

    oni: {
      name: 'Oni',
      desc: 'Iblis merah bertanduk dengan mata celah menyala. Galak, gelap, penuh amarah.',
      idle: { soul: '#ff2a2a', eye: '#ffb21f' },
      gaze: [1.3, 0.3],
      svg: id => {
        const L = 'M8.6 18.3Q11.5 16.5 14.3 18.5Q11.4 20.9 8.6 18.3Z';
        const R = 'M23.4 18.3Q20.5 16.5 17.7 18.5Q20.6 20.9 23.4 18.3Z';
        return `<defs>
          <radialGradient id="${id}-skin" cx="50%" cy="34%" r="72%"><stop offset="0%" stop-color="#c8202c"/><stop offset="55%" stop-color="#7c0a16"/><stop offset="100%" stop-color="#2a0206"/></radialGradient>
          <linearGradient id="${id}-horn" x1="0" y1="1" x2="0" y2="0"><stop offset="0%" stop-color="#e8dcc4"/><stop offset="70%" stop-color="#b9a582"/><stop offset="100%" stop-color="#2b1a12"/></linearGradient>
          <clipPath id="${id}-l"><path d="${L}"/></clipPath><clipPath id="${id}-r"><path d="${R}"/></clipPath>
        </defs>
        <g class="head"><g class="body">
          <path d="M10.2 11.6C8.2 9.6 6.3 6.3 5.4 1.9c2.8 2.3 5.7 5 7.3 7.7z" fill="url(#${id}-horn)"/>
          <path d="M21.8 11.6c2-2 3.9-5.3 4.8-9.7-2.8 2.3-5.7 5-7.3 7.7z" fill="url(#${id}-horn)"/>
          <path d="M6.2 13.2C7.8 10.1 11.5 8.6 16 8.6s8.2 1.5 9.8 4.6l-.4 6.4c-.6 4.3-3.7 8.1-9.4 10.6-5.7-2.5-8.8-6.3-9.4-10.6z" fill="url(#${id}-skin)"/>
          <path d="M8.2 21.1l2.4 1.4M23.8 21.1l-2.4 1.4M12.3 12.2l1.1 1.6M19.7 12.2l-1.1 1.6" stroke="#240207" stroke-width="0.75" stroke-linecap="round"/>
          <path class="glow" d="M16 10l.9 2.2L16 14.1l-.9-1.9z"/>
          <path d="M7.9 14.8l6.4 2.3-.6 1.1-5.6-1.8zM24.1 14.8l-6.4 2.3.6 1.1 5.6-1.8z" fill="#140104"/>
          <g class="eyes">
            <path d="${L}" fill="#0d0103"/><path d="${R}" fill="#0d0103"/>
            <g clip-path="url(#${id}-l)"><g class="iris"><circle class="glow" cx="11.5" cy="18.6" r="1.25"/><ellipse cx="11.5" cy="18.6" rx="0.28" ry="1.05" fill="#0d0103"/></g></g>
            <g clip-path="url(#${id}-r)"><g class="iris"><circle class="glow" cx="20.5" cy="18.6" r="1.25"/><ellipse cx="20.5" cy="18.6" rx="0.28" ry="1.05" fill="#0d0103"/></g></g>
          </g>
          <path d="M10.3 23q5.7 2.6 11.4 0-1 3.7-5.7 4.5-4.7-.8-5.7-4.5z" fill="#0d0103"/>
          <path d="M12.1 23.6l.8 2.1.7-1.6zM19.9 23.6l-.8 2.1-.7-1.6zM13.9 27.1l.5-1.7.6 1.8zM18.1 27.1l-.5-1.7-.6 1.8z" fill="#efe2c8"/>
        </g></g>`;
      },
    },

    kitsune: {
      name: 'Kitsune',
      desc: 'Topeng roh rubah. Putih bersih dengan tanda merah, licik tapi setia.',
      idle: { soul: '#ff4d5e', eye: '#ffcf5a' },
      gaze: [0.9, 0.25],
      svg: id => {
        const L = 'M9.1 16.6Q11.6 15.1 14 17.3Q11.4 18.1 9.1 16.6Z';
        const R = 'M22.9 16.6Q20.4 15.1 18 17.3Q20.6 18.1 22.9 16.6Z';
        return `<defs>
          <radialGradient id="${id}-skin" cx="50%" cy="38%" r="70%"><stop offset="0%" stop-color="#ffffff"/><stop offset="65%" stop-color="#eeeae2"/><stop offset="100%" stop-color="#a9a297"/></radialGradient>
          <clipPath id="${id}-l"><path d="${L}"/></clipPath><clipPath id="${id}-r"><path d="${R}"/></clipPath>
        </defs>
        <g class="head"><g class="body">
          <path d="M6.8 13.4 8 2.6l6 7.4z" fill="url(#${id}-skin)"/><path d="M25.2 13.4 24 2.6l-6 7.4z" fill="url(#${id}-skin)"/>
          <path d="M8.7 10.6 9.3 5.4l3.1 4z" fill="#d6283a"/><path d="M23.3 10.6l-.6-5.2-3.1 4z" fill="#d6283a"/>
          <path d="M6.3 12.4C7.9 9.5 11.6 8.4 16 8.4s8.1 1.1 9.7 4c.7 3.4-.7 6.8-3.1 9.4-2 2.4-4 5-6.6 7.9-2.6-2.9-4.6-5.5-6.6-7.9-2.4-2.6-3.8-6-3.1-9.4z" fill="url(#${id}-skin)"/>
          <path d="M16 9.4c1.3 1.9 1.1 3.6 0 5.1-1.1-1.5-1.3-3.2 0-5.1z" fill="#d6283a"/>
          <path d="M8.3 14.9q2.3.4 3.1 2.6M23.7 14.9q-2.3.4-3.1 2.6" stroke="#d6283a" stroke-width="0.7" fill="none" stroke-linecap="round"/>
          <g class="eyes">
            <path d="${L}" fill="#160608"/><path d="${R}" fill="#160608"/>
            <g clip-path="url(#${id}-l)"><circle class="iris glow" cx="11.8" cy="16.6" r="0.75"/></g>
            <g clip-path="url(#${id}-r)"><circle class="iris glow" cx="20.2" cy="16.6" r="0.75"/></g>
            <path d="M8.7 16.2q2.9-1.9 5.6.9M23.3 16.2q-2.9-1.9-5.6.9" stroke="#d6283a" stroke-width="0.55" fill="none" stroke-linecap="round"/>
          </g>
          <path d="M14.6 21.4q1.4-.6 2.8 0M13.9 19.6l-.9 2.2M18.1 19.6l.9 2.2" stroke="#b9b2a6" stroke-width="0.35" fill="none" stroke-linecap="round"/>
          <circle cx="11.9" cy="23.4" r="0.45" fill="#d6283a"/><circle cx="20.1" cy="23.4" r="0.45" fill="#d6283a"/>
          <circle cx="12.9" cy="24.6" r="0.35" fill="#d6283a"/><circle cx="19.1" cy="24.6" r="0.35" fill="#d6283a"/>
          <ellipse cx="16" cy="26.4" rx="1.05" ry="0.75" fill="#160608"/>
          <path d="M14.7 27.8q1.3.8 2.6 0" stroke="#160608" stroke-width="0.4" fill="none" stroke-linecap="round"/>
        </g></g>`;
      },
    },
  };

  window.SHIKI_MASKS = MASKS;

  window.maskSvg = (mask, big) => {
    const m = MASKS[mask] || MASKS.hannya;
    const id = `mask-${uid++}`;
    return `<svg class="soul${big ? ' big' : ''}" viewBox="0 0 32 32" data-gx="${m.gaze[0]}" data-gy="${m.gaze[1]}" aria-hidden="true">${m.svg(id)}</svg>`;
  };

  // Idle colours of the chosen mask (states override them).
  window.applyMaskColours = mask => {
    const m = MASKS[mask] || MASKS.hannya;
    document.documentElement.style.setProperty('--soul', m.idle.soul);
    document.documentElement.style.setProperty('--eye', m.idle.eye);
    document.documentElement.dataset.mask = mask;
  };

  // Eyes follow a point (page coordinates): irises slide within their sockets, the head turns a little.
  // The loop runs only while the point keeps moving.
  let target = null, until = 0, running = false;
  const gaze = new WeakMap();
  function frame(now) {
    for (const svg of document.querySelectorAll('svg.soul')) {
      const r = svg.getBoundingClientRect();
      if (!r.width) continue;
      const dx = target[0] - (r.left + r.width / 2), dy = target[1] - (r.top + r.height * 0.58);
      const d = Math.hypot(dx, dy) || 1, k = Math.min(1, d / 220);
      const g = gaze.get(svg) || { x: 0, y: 0 };
      g.x += (dx / d * k - g.x) * 0.16;
      g.y += (dy / d * k - g.y) * 0.16;
      gaze.set(svg, g);
      const gx = +svg.dataset.gx || 1, gy = +svg.dataset.gy || 0.5;
      svg.querySelectorAll('.iris').forEach(i => (i.style.transform = `translate(${g.x * gx}px, ${g.y * gy}px)`));
      const head = svg.querySelector('.head');
      if (head) head.style.transform = `translate(${g.x * 0.5}px, ${g.y * 0.35}px) rotate(${g.x * 6}deg)`;
    }
    if (now < until) requestAnimationFrame(frame); else running = false;
  }
  window.lookAt = (x, y) => {
    target = [x, y];
    until = performance.now() + 1200;
    if (!running) { running = true; requestAnimationFrame(frame); }
  };

  // Base animation for every mask, wherever it is shown.
  const css = document.createElement('style');
  css.textContent = `
    .soul { width: 26px; height: 26px; flex: none; overflow: visible; filter: drop-shadow(0 0 4px color-mix(in srgb, var(--soul) 80%, transparent)); }
    .soul .head, .soul .body, .soul .eyes, .soul .iris { transform-box: view-box; }
    .soul .head { transform-origin: 16px 20px; }
    .soul .body { animation: soul-breathe 4s ease-in-out infinite; transform-origin: 16px 28px; }
    .soul .eyes { animation: soul-blink 6s infinite; transform-origin: 16px 17px; }
    .soul .glow { fill: var(--eye); filter: drop-shadow(0 0 0.8px var(--eye)); transition: fill 240ms ease; }
    @keyframes soul-breathe { 50% { transform: translateY(-0.6px) scale(1.012); } }
    @keyframes soul-blink { 0%, 93%, 100% { transform: scaleY(1); } 95.5% { transform: scaleY(0.08); } }
    @media (prefers-reduced-motion: reduce) { .soul .body, .soul .eyes { animation: none; } }
  `;
  document.head.append(css);
})();
