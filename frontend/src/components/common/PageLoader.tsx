// src/components/common/PageLoader.tsx
import { Spinner } from "@/components/ui/spinner";

/**
 * Full-height placeholder for a route that cannot decide what to render yet.
 *
 * Used by the guards while they resolve their question — is the instance set
 * up, is anyone signed in. Deliberately bare: it is on screen for a few hundred
 * milliseconds, and a skeleton of a page we may redirect away from would be a
 * lie. `Spinner` carries the `role="status"`, so this wrapper stays presentational.
 */
export function PageLoader() {
  return (
    <div className="grid min-h-dvh place-items-center">
      <Spinner className="size-6 text-muted-foreground" />
    </div>
  );
}
