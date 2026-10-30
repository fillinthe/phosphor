// The loading screen's progress (trunk's initializer hooks, see index.html). The download is counted against the
// app's real size; after that the browser compiles it and the app starts ("Starting…"), and the app itself fades the
// screen out on its first frame (main.rs).
export default function loader() {
  const bar = () => document.getElementById("loading-fill");
  const say = (text) => {
    const s = document.getElementById("loading-status");
    if (s) s.textContent = text;
  };
  return {
    onStart: () => say("Downloading"),
    onProgress: ({ current, total }) => {
      const b = bar();
      if (b && total > 0) b.style.transform = `scaleX(${Math.min(current / total, 1)})`;
    },
    onComplete: () => {
      const b = bar();
      if (b) b.style.transform = "scaleX(1)";
      document.getElementById("loading")?.classList.add("starting");
      say("Starting…");
    },
    onFailure: () => say("Phosphor couldn't load. Reload the page to try again."),
  };
}
