// The links of the text of an agent on a phone: an address of the loopback interface
// means the Mac, not the phone.

/** An address of the loopback interface. */
export function isLoopback(href: string): boolean {
  try {
    const host = new URL(href).hostname;
    return host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '::1';
  } catch {
    return false;
  }
}
