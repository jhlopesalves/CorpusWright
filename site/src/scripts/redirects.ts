const target = document.documentElement.dataset.redirectTarget;
if (target) {
  window.location.replace(`${target}${window.location.hash}`);
}
