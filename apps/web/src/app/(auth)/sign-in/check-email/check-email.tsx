"use client";

import { EnvelopeIcon, Icon } from "@ci/ui/icons";
import { motion, spring, useReducedMotion } from "@ci/ui/motion";
import { Button } from "@ci/ui/primitives";
import Link from "next/link";

/**
 * After a link is requested. The same words whether or not the address may sign in (ADR-WEB-3),
 * so this page never reveals an account. `email` comes only from the proxy (never the URL); without
 * it the line stays generic. The envelope lands once on `land` (M1: it says the
 * thing was sent).
 */
export function CheckEmail({ email }: { email: string | null }) {
  const reduced = useReducedMotion();
  return (
    <div className="flex flex-col gap-8">
      <motion.div
        aria-hidden="true"
        className="flex size-14 items-center justify-center rounded-pill bg-well text-ink"
        initial={reduced ? false : { scale: 0.6, y: -8, opacity: 0 }}
        animate={{ scale: 1, y: 0, opacity: 1 }}
        transition={spring.land}
      >
        <Icon icon={EnvelopeIcon} size={24} />
      </motion.div>
      <div className="flex flex-col gap-3">
        <h1 id="auth-title" className="font-serif text-3xl font-semibold text-ink">
          Check your email
        </h1>
        <p className="text-lg text-ink-soft">
          {`We sent a sign-in link to ${email ?? "your email"}. It expires in 10 minutes.`}
        </p>
      </div>
      <p className="text-sm text-ink-faint">
        Nothing there? Check your spam folder, or request a new link.
      </p>
      <Button asChild variant="outline" className="w-fit">
        <Link href="/sign-in">Use a different email</Link>
      </Button>
    </div>
  );
}
