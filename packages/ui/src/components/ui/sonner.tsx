import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from 'lucide-react';
import { Toaster as Sonner, type ToasterProps } from 'sonner';

// neohar: the chrome is dark only, so the toaster has no theme switch, and its colours are the
// tokens of the glass that floats over the panes.
const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      theme="dark"
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      toastOptions={{
        // The icon has the colour of its tone: green is live, amber waits, red fails.
        classNames: {
          success: '[&_[data-icon]]:text-live',
          warning: '[&_[data-icon]]:text-caution',
          error: '[&_[data-icon]]:text-destructive',
        },
      }}
      style={
        {
          '--normal-bg': 'var(--glass)',
          '--normal-text': 'var(--text)',
          '--normal-border': 'var(--hairline)',
          '--border-radius': 'var(--r-md)',
        } as React.CSSProperties
      }
      {...props}
    />
  );
};

export { Toaster };
