/** Enhances the static walkthrough with a second, visual-only figure stage. */
(() => {
  const tour = document.querySelector('.walkthrough');
  if (!tour || !window.IntersectionObserver || !window.ResizeObserver) return;

  const root = document.documentElement;
  const header = document.querySelector('.site-header');
  const hero = tour.querySelector('.walk-hero');
  const stage = tour.querySelector('.walkthrough-stage');
  const stageMedia = stage.querySelector('.walk-stage-media');
  const stageCaption = stage.querySelector('.walk-stage-caption');
  const wide = window.matchMedia('(min-width: 1000px)');
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const steps = [...tour.querySelectorAll('[data-walk-step]')].map(section => ({
    key: section.dataset.walkStep,
    heading: section.querySelector('h2'),
    figure: section.querySelector('figure'),
    fullImage: section.querySelector('.walk-full-image'),
    distributionLink: section.querySelector('.walk-distribution-link'),
  }));
  const intro = { key: 'hero', figure: hero.querySelector('figure') };
  const preloaded = new Set();
  let active = null;
  let observer;
  let frame = 0;
  let activationY = 0;
  let viewportHeight = 0;

  function darkTheme() {
    return root.dataset.theme === 'dark';
  }

  function syncPictures() {
    for (const source of tour.querySelectorAll('picture source')) {
      source.media = darkTheme() ? 'all' : 'not all';
    }
    for (const step of steps) {
      if (step.fullImage) step.fullImage.href = pictureUrl(step);
      if (step.distributionLink) step.distributionLink.href = pictureUrl(step, '.walk-distribution');
    }
  }

  function pictureUrl(step, selector = 'picture') {
    const picture = step?.figure.querySelector(selector);
    if (!picture) return null;
    return darkTheme() ? picture.querySelector('source').srcset : picture.querySelector('img').src;
  }

  function preload(step) {
    const url = pictureUrl(step);
    if (!url || preloaded.has(url)) return;
    preloaded.add(url);
    new Image().src = url;
  }

  function show(step) {
    if (active === step) return;
    active = step;

    const layer = document.createElement('div');
    layer.className = 'walk-stage-layer';
    const media = step.figure.querySelector('picture, pre').cloneNode(true);
    const image = media.querySelector('img');
    if (image) {
      image.loading = 'eager';
      image.removeAttribute('fetchpriority');
    }
    layer.append(media);

    const previous = stageMedia.querySelector('[data-stage-current]');
    for (const child of [...stageMedia.children]) {
      if (child !== previous) child.remove();
    }
    stageMedia.append(layer);
    syncPictures();
    function display() {
      if (!layer.isConnected || active !== step) return;
      stage.dataset.activeStep = step.key;
      const caption = step.figure.querySelector('figcaption').cloneNode(true);
      stageCaption.replaceChildren(...caption.childNodes);

      // Commit the layer’s starting opacity before changing its state.
      if (previous && !reducedMotion.matches) layer.getBoundingClientRect();
      layer.setAttribute('data-stage-current', '');
      if (!previous) return;
      previous.removeAttribute('data-stage-current');
      if (reducedMotion.matches || getComputedStyle(previous).opacity === '0') {
        previous.remove();
      } else {
        previous.addEventListener('transitionend', () => previous.remove(), { once: true });
      }
    }
    // Hold the previous figure during a long jump until the new capture is available.
    if (previous && image && !image.complete) {
      image.addEventListener('load', display, { once: true });
      image.addEventListener('error', display, { once: true });
    } else {
      display();
    }
  }

  function update() {
    frame = 0;
    const heroBottom = hero.getBoundingClientRect().bottom;
    header.classList.toggle('is-solid', heroBottom <= header.offsetHeight);
    const index = steps.findLastIndex(step => step.heading.getBoundingClientRect().top <= activationY);
    show(steps[index] ?? intro);

    const next = steps[index + 1];
    if (next && next.heading.getBoundingClientRect().top < window.innerHeight * 1.4) preload(next);
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(update);
  }

  function measure() {
    const headerHeight = header.offsetHeight;
    const stageHeight = stage.offsetHeight;
    document.body.style.setProperty('--site-header-height', `${headerHeight}px`);
    tour.style.setProperty('--stage-height', `${stageHeight}px`);
    // Leave room for the step label above the mobile heading.
    const y = wide.matches
      ? Math.round(window.innerHeight / 2)
      : Math.min(window.innerHeight - 32, stageHeight + 44);
    schedule();
    if (observer && activationY === y && viewportHeight === window.innerHeight) return;
    activationY = y;
    viewportHeight = window.innerHeight;
    stage.dataset.activationY = String(y);

    observer?.disconnect();
    // Percent root margins resolve against the viewport width. Use measured
    // pixels so the band follows the reading line on both phones and desktops.
    observer = new IntersectionObserver(schedule, {
      rootMargin: `-${Math.max(0, y - 12)}px 0px -${Math.max(0, window.innerHeight - y - 12)}px 0px`,
    });
    for (const step of steps) observer.observe(step.heading);
    observer.observe(hero);
  }

  tour.classList.add('is-enhanced');
  stage.hidden = false;
  syncPictures();
  show(intro);
  measure();

  const resize = new ResizeObserver(measure);
  resize.observe(stage);
  resize.observe(header);
  window.addEventListener('resize', measure);
  // A jump can move a heading completely across the observer band in one frame.
  window.addEventListener('scroll', schedule, { passive: true });
  new MutationObserver(() => {
    syncPictures();
    schedule();
  }).observe(root, { attributes: true, attributeFilter: ['data-theme'] });
})();
