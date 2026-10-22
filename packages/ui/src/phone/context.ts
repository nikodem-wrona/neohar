// What every view of the phone app reads: the transport of the Mac, the stores of the phone, this
// phone and the machine that the desk shows.

import type { PhoneAccount, PhoneDevice, Transport } from '@neohar/transport';
import { createContext, useContext } from 'solid-js';
import type { PhoneStores } from '../stores';
import type { PushBrowser } from './push';

export interface Phone {
  /** The transport of the Mac, where the phone is paired, whatever machine the desk shows. */
  transport: Transport;
  stores: PhoneStores;
  account: PhoneAccount;
  /** This phone, as the listener knows it. */
  device: PhoneDevice;
  /** The push of the browser, for the switch Notifications. */
  push: PushBrowser;
  /** The machine that the desk shows: `local` for the Mac itself. */
  machine: string;
  /** Shows the desk of another machine of the Mac, or of the Mac with `local`. */
  showMachine: (machine: string) => void;
}

const PhoneContext = createContext<Phone>();

export const PhoneProvider = PhoneContext.Provider;

export function usePhone(): Phone {
  const phone = useContext(PhoneContext);
  if (!phone) throw new Error('a view of the phone reads the phone outside of its provider');
  return phone;
}
