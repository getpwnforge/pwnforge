import { Turnstile, type TurnstileInstance } from "@marsidev/react-turnstile";
import type { RefObject } from "react";

interface TurnstileWidgetProps {
  siteKey: string | undefined;
  widgetRef: RefObject<TurnstileInstance | null>;
  onSuccess: (token: string) => void;
}

export function TurnstileWidget({ siteKey, widgetRef, onSuccess }: Readonly<TurnstileWidgetProps>) {
  if (!siteKey) return null;
  return <Turnstile ref={widgetRef} siteKey={siteKey} onSuccess={onSuccess} />;
}
