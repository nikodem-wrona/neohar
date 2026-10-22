// The entry of the phone app. `apps/web` imports this module and not the index of the
// package, so its first chunk has no surface, no WebGL and no WASM: the desk loads them after its
// first paint.

export { realTimers } from '../lib/timing';
export { ATLAS_SLOTS } from '../surface/geometry';
export { mountPhone } from './mount';
export type { PhoneTransport } from './PhoneApp';
