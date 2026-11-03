const toggle = document.querySelector(".nav-toggle");
const nav = document.querySelector("#site-nav");

toggle?.addEventListener("click", () => {
  const open = nav?.classList.toggle("is-open");
  toggle.setAttribute("aria-expanded", open ? "true" : "false");
});

nav?.querySelectorAll("a").forEach((link) => {
  link.addEventListener("click", () => {
    nav.classList.remove("is-open");
    toggle?.setAttribute("aria-expanded", "false");
  });
});

const copy = document.querySelector("#copy-cmd");
const command = document.querySelector("#install-cmd");
const status = document.querySelector("#copy-status");

copy?.addEventListener("click", async () => {
  const text = command?.textContent?.trim() ?? "";
  try {
    await navigator.clipboard.writeText(text);
    if (status) status.textContent = "Copied.";
  } catch {
    if (status) status.textContent = "Select the command and copy it.";
  }
});
