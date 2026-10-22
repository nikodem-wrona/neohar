// The public surface of the UI package. The app mounts it with a transport.

export { ATLAS_SLOTS } from './surface';
export { mountApp, mountSettings } from './app/mount';
export type { MountedApp, MountOptions } from './app/mount';
export { isWindowId, parseParams } from './app/params';
export { realTimers } from './lib/timing';
export { mountPhone } from './phone/mount';
export type { PhoneTransport } from './phone/PhoneApp';
export type { AppParams, View } from './app/params';
