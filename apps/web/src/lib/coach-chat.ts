/**
 * Whether the coach chat pages (`/app/coach`, M5b Task 9) are live. While false, nothing links to
 * them: the header hides Coach, the command menu drops "Ask your coach", and the report, weakness
 * and area pages leave out their "Ask your coach" links (M5b T11). On since Task 9 merged; it is
 * the one place to take the chat out of the product again.
 */
export function coachChatLive(): boolean {
  return true;
}
