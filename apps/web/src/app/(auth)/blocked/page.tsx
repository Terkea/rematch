import { AuthPage } from "../auth-page";
import { BlockedNotice } from "./blocked-notice";

export const metadata = { title: "Not available" };

/** Where the age gate sends an under-13 after deleting the account (ADR-WEB-6). */
export default function BlockedPage() {
  return (
    <AuthPage aside="start">
      <BlockedNotice />
    </AuthPage>
  );
}
