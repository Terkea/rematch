"use client";

import { createContext, type ReactNode, useContext } from "react";
import { type StreamMode, useEventStream } from "./use-event-stream";

const RealtimeContext = createContext<{ mode: StreamMode; lastEventId: number }>({
  mode: "off",
  lastEventId: 0,
});

/**
 * Mounts the one event stream for a signed-in user (M4 T23); render it only inside the signed-in
 * shell. `accountIds` are the user's linked accounts as the server read them: their joined ids
 * are the `reconnectKey`, so linking or removing an account reopens the stream (the server
 * re-reads which accounts the session may see). `initialEventId` is the head event id read on
 * the server before the page data, so nothing stored in between is lost.
 */
export function RealtimeProvider({
  accountIds,
  initialEventId,
  children,
}: {
  accountIds: string[];
  initialEventId: number | null;
  children: ReactNode;
}) {
  const stream = useEventStream({
    reconnectKey: [...accountIds].sort().join(","),
    since: initialEventId,
  });
  return <RealtimeContext.Provider value={stream}>{children}</RealtimeContext.Provider>;
}

/** How updates arrive right now. Never call it "live" unless `mode` is "sse". */
export function useRealtime(): { mode: StreamMode; lastEventId: number } {
  return useContext(RealtimeContext);
}
