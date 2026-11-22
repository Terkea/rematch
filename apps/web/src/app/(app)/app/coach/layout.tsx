import type { ReactNode } from "react";

// The coach chat's frame (Task 9): one column of pages that share the back link, the list and
// the thread. Each page reads its own data and checks the user itself.

export const metadata = { title: { template: "%s · Ask your coach", default: "Ask your coach" } };

export default function CoachLayout({ children }: { children: ReactNode }) {
  return <div className="flex flex-1 flex-col gap-8">{children}</div>;
}
