import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { JOB } from "../../src/server/jobs";

// R5 (ADR 0070): the web inserts the rows and the `coach-chat` job; the worker answers and its
// events arrive on /api/events. These pin that contract against the Rust sources.
const repo = (path: string) =>
  readFileSync(new URL(`../../../../${path}`, import.meta.url), "utf8");

describe("chat hand-off to the worker", () => {
  it("the job is the one Rust post_user_message enqueues", () => {
    const threads = repo("backend/crates/ai/src/chat/threads.rs");
    expect(threads).toContain("enqueue_chat(queue, assistant.id, account_id)");
    const kinds = repo("backend/crates/ai/src/jobs/kinds.rs");
    // enqueue_chat: CoachChatPayload { message_id }, chat_key, user_key = the account.
    expect(kinds).toMatch(
      /pub async fn enqueue_chat\([^)]*\)[^{]*\{\s*let payload = CoachChatPayload \{ message_id \};\s*let key = chat_key\(message_id\);\s*enqueue_one\(\s*queue,\s*KIND_COACH_CHAT,\s*&payload,\s*key,\s*account_id,\s*CHAT_PRIORITY,\s*Some\(CHAT_MAX_ATTEMPTS\),/,
    );
    expect(kinds).toContain("user_key: Some(account_id)");
    expect(JOB.chat.payload("m")).toEqual({ messageId: "m" });
    expect(JOB.chat.key("m")).toBe("chat:m");
  });

  it("the job answers the pending assistant message by id", () => {
    const job = repo("backend/crates/ai/src/chat/job.rs");
    expect(job).toMatch(/let payload: CoachChatPayload =\s*serde_json::from_value\(job\.payload\)/);
    expect(job).toContain("let id = payload.message_id;");
  });

  it("the chat events are user-owned and named as the web reads them", () => {
    const events = repo("backend/crates/ai/src/events.rs");
    for (const [name, value] of [
      ["CHAT_CHUNK", "chat.chunk"],
      ["CHAT_MESSAGE_READY", "chat.message.ready"],
      ["CHAT_MESSAGE_FAILED", "chat.message.failed"],
      ["REPORT_SECTION_READY", "report.section.ready"],
      ["REPORT_READY", "report.ready"],
      ["REPORT_DEGRADED", "report.degraded"],
      ["WEEKLY_REVIEW_READY", "weekly_review.ready"],
      ["WEAKNESS_NOTE_READY", "weakness.note.ready"],
    ]) {
      expect(events).toContain(`pub const ${name}: &str = "${value}";`);
    }
    for (const name of ["CHAT_CHUNK", "CHAT_MESSAGE_READY", "CHAT_MESSAGE_FAILED"]) {
      expect(events).toContain(`(${name}, Audience::User)`);
    }
    // camelCase payload fields the reducers and the chunk replay read.
    expect(events).toMatch(
      /pub struct ChatChunk \{\s*pub message_id: Uuid,\s*pub seq: u32,\s*pub text: String,\s*pub segments: Vec<Segment>,/,
    );
    expect(events).toMatch(
      /pub struct ChatReady \{\s*pub message_id: Uuid,[^}]*pub replace: bool,/,
    );
    expect(events).toMatch(
      /pub struct ChatFailed \{\s*pub message_id: Uuid,\s*pub reason: String,/,
    );
  });

  it("the web never holds the sidecar's address or token", () => {
    const compose = repo("docker-compose.yml");
    const webEnv = compose.slice(
      compose.indexOf("x-web-environment:"),
      compose.indexOf("\nservices:"),
    );
    expect(webEnv.length).toBeGreaterThan(100);
    expect(webEnv).not.toMatch(/SIDECAR/i);
    expect(repo("apps/web/src/env.ts")).not.toMatch(/SIDECAR/i);
  });
});
