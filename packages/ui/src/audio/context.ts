/** The shared AudioContext, made lazily so server render and tests never touch it. */
export function defaultContextFactory(): AudioContext {
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) throw new Error("Web Audio is not available");
  return new Ctor({ latencyHint: "interactive" });
}
