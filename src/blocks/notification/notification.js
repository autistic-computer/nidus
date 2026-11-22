(() => {
  function dismiss(toast) {
    if (!toast || toast.hasAttribute("data-still") || toast.hasAttribute("data-closing")) return;
    toast.setAttribute("data-closing", "");
    setTimeout(() => toast.remove(), 150);
  }

  document.addEventListener("click", (event) => {
    const close = event.target.closest(".penna-toast-close > button");
    if (close) {
      event.stopPropagation();
      dismiss(close.closest(".penna-toast"));
      return;
    }
    const message = event.target.closest(".penna-toast-message");
    if (message && !event.target.closest("button, a")) {
      message.toggleAttribute("data-expanded");
    }
  });

  document.addEventListener("auxclick", (event) => {
    if (event.button !== 1) return;
    const toast = event.target.closest(".penna-toast");
    if (toast) dismiss(toast);
  });

  window.pennaToast = { dismiss };
})();
