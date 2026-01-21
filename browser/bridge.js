// Between the page (where the player lives) and the extension (which talks to kestrel).
window.addEventListener("message", (e) => {
  if (e.source === window && e.data && e.data.__kestrel === "state") {
    chrome.runtime.sendMessage({ state: e.data.state }).catch(() => {});
  }
});
chrome.runtime.onMessage.addListener((m) => {
  window.postMessage({ __kestrel: "cmd", cmd: m.cmd, arg: m.arg }, "*");
});
