// Keeps what each tab plays and hands kestrel the one that matters: the tab playing now, else
// the one that played last. kestrel's commands go back to that tab.
const tabs = new Map();
let port = null;

function best() {
  let pick = null;
  for (const [tab, s] of tabs) {
    if (!pick || (s.playing && !pick.s.playing) || (s.playing === pick.s.playing && s.at > pick.s.at)) {
      pick = { tab, s };
    }
  }
  return pick;
}

function publish() {
  if (!port) return;
  const p = best();
  port.postMessage(p ? { tab: p.tab, ...p.s } : { tab: null });
}

function connect() {
  port = chrome.runtime.connectNative("dev.kestrel.media");
  port.onMessage.addListener((m) => {
    const p = best();
    if (p && m.cmd) chrome.tabs.sendMessage(p.tab, m).catch(() => {});
  });
  port.onDisconnect.addListener(() => {
    port = null;
    setTimeout(connect, 5000);
  });
  publish();
}

chrome.runtime.onMessage.addListener((m, sender) => {
  const tab = sender.tab && sender.tab.id;
  if (tab == null) return;
  if (m.state) tabs.set(tab, { ...m.state, at: Date.now() });
  else tabs.delete(tab);
  publish();
});
chrome.tabs.onRemoved.addListener((tab) => {
  tabs.delete(tab);
  publish();
});

connect();
