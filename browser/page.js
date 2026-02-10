// What this page is playing, read from the page itself: Apple Music's own player API on
// music.apple.com, the standard media session and the playing <audio>/<video> elsewhere.
(() => {
  const music = () => {
    try {
      return location.host === "music.apple.com" && window.MusicKit ? MusicKit.getInstance() : null;
    } catch {
      return null;
    }
  };

  const element = () => {
    const all = [...document.querySelectorAll("video, audio")].filter((m) => m.duration > 0);
    return all.find((m) => !m.paused) || all.sort((a, b) => b.duration - a.duration)[0] || null;
  };

  const art = (url) => (url ? url.replace("{w}", "512").replace("{h}", "512").replace("{f}", "jpg") : "");

  const read = () => {
    const mk = music();
    if (mk && mk.nowPlayingItem) {
      const it = mk.nowPlayingItem;
      const a = it.attributes || {};
      return {
        title: it.title || a.name || "",
        artist: it.artistName || a.artistName || "",
        album: it.albumName || a.albumName || "",
        art: art((a.artwork && a.artwork.url) || it.artworkURL || ""),
        playing: mk.isPlaying === true || mk.playbackState === 2,
        position: mk.currentPlaybackTime || 0,
        duration: mk.currentPlaybackDuration || (a.durationInMillis || 0) / 1000,
        canNext: true,
        canPrev: true,
        site: "Apple Music",
      };
    }
    const el = element();
    if (!el) return null;
    const md = navigator.mediaSession && navigator.mediaSession.metadata;
    const artwork = md && md.artwork && md.artwork.length ? md.artwork[md.artwork.length - 1].src : "";
    return {
      title: (md && md.title) || document.title,
      artist: (md && md.artist) || "",
      album: (md && md.album) || "",
      art: artwork,
      playing: !el.paused,
      position: el.currentTime || 0,
      duration: Number.isFinite(el.duration) ? el.duration : 0,
      canNext: !!document.querySelector(".ytp-next-button"),
      canPrev: false,
      site: location.host.replace(/^www\./, ""),
    };
  };

  let last = "";
  const send = (force) => {
    const s = read();
    const key = JSON.stringify(s && { ...s, position: Math.floor((s.position || 0) / 5) });
    if (!force && key === last) return;
    last = key;
    window.postMessage({ __kestrel: "state", state: s }, "*");
  };

  // Changes are sent as they happen; while playing, the position is refreshed every 5 s so
  // kestrel's timeline stays exact (it moves the bar itself in between).
  ["play", "pause", "seeked", "durationchange", "loadedmetadata", "emptied", "ended"].forEach((ev) =>
    document.addEventListener(ev, () => send(true), true)
  );
  setInterval(() => send(false), 1000);
  // Every 5 s regardless, so a helper that restarted (or a worker that slept) catches up.
  setInterval(() => send(true), 5000);
  const hook = () => {
    const mk = music();
    if (!mk || mk.__kestrel) return;
    mk.__kestrel = true;
    ["playbackStateDidChange", "nowPlayingItemDidChange", "playbackTimeDidChange"].forEach((ev) =>
      mk.addEventListener(ev, () => send(ev !== "playbackTimeDidChange"))
    );
  };
  setInterval(hook, 2000);

  window.addEventListener("message", (e) => {
    if (e.source !== window || !e.data || e.data.__kestrel !== "cmd") return;
    const { cmd, arg } = e.data;
    const mk = music();
    const el = element();
    if (mk && mk.nowPlayingItem) {
      if (cmd === "playpause") (mk.isPlaying ? mk.pause() : mk.play());
      if (cmd === "play") mk.play();
      if (cmd === "pause") mk.pause();
      if (cmd === "next") mk.skipToNextItem();
      if (cmd === "previous") mk.skipToPreviousItem();
      if (cmd === "seek") mk.seekToTime(arg);
    } else if (el) {
      if (cmd === "playpause") (el.paused ? el.play() : el.pause());
      if (cmd === "play") el.play();
      if (cmd === "pause") el.pause();
      if (cmd === "seek") el.currentTime = arg;
      if (cmd === "next") document.querySelector(".ytp-next-button")?.click();
    }
    setTimeout(() => send(true), 150);
  });
})();
