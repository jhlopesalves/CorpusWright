(function () {
  'use strict';

  var gallery = document.querySelector('[data-product-gallery]');
  if (!gallery) return;
  var navigation = gallery.querySelector('.gallery-tabs');
  var tabs = Array.from(gallery.querySelectorAll('[data-gallery-tab]'));
  var panels = Array.from(gallery.querySelectorAll('[data-gallery-panel]'));
  if (tabs.length !== 3 || panels.length !== tabs.length) return;

  function select(index, focus) {
    tabs.forEach(function (tab, position) {
      tab.setAttribute('aria-selected', String(position === index));
      tab.tabIndex = position === index ? 0 : -1;
      panels[position].hidden = position !== index;
    });
    if (focus) tabs[index].focus();
  }

  navigation.setAttribute('role', 'tablist');
  tabs.forEach(function (tab, index) {
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', panels[index].id);
    panels[index].setAttribute('role', 'tabpanel');
    panels[index].setAttribute('aria-labelledby', tab.id);
    panels[index].tabIndex = 0;
    tab.addEventListener('click', function () { select(index, false); });
    tab.addEventListener('keydown', function (event) {
      var next;
      if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
      else if (event.key === 'ArrowLeft') next = (index + tabs.length - 1) % tabs.length;
      else if (event.key === 'Home') next = 0;
      else if (event.key === 'End') next = tabs.length - 1;
      else return;
      event.preventDefault();
      select(next, true);
    });
  });
  // No timing or motion: visitors choose how long to inspect each application state.
  select(0, false);
  navigation.hidden = false;
})();
