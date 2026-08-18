/**
 * How an observed application is drawn, wherever it is drawn.
 *
 * Activity and the work reports both plot app usage, and they have to agree:
 * the colour rule below is a *stable hash* precisely so an app keeps its colour
 * from one day to the next, and two screens computing it two ways would throw
 * away the property the hash exists to provide.
 */

/** The eight `--app-*` tokens from §5.2. */
const APP_RAMP = 8;

/**
 * Which token an application gets.
 *
 * A stable hash rather than order-of-appearance, so an app keeps its colour
 * from one day to the next — a legend you have to re-learn every morning is
 * worse than no colour at all.
 */
export function appColour(appId: string): string {
  let hash = 0;
  for (let i = 0; i < appId.length; i++) hash = (hash * 31 + appId.charCodeAt(i)) | 0;
  return `var(--app-${(Math.abs(hash) % APP_RAMP) + 1})`;
}

/** `code.exe` and `Code.app` are the same thing to a human reading a total. */
export function appLabel(appId: string): string {
  return appId.replace(/\.(exe|app)$/i, "");
}
