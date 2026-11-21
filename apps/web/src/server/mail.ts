import nodemailer, { type Transporter } from "nodemailer";
import { brand } from "../brand";
import { getEnv } from "../env";

export type MailMessage = { to: string; subject: string; text: string; html: string };

let transport: Transporter | undefined;

/** Sends one message over `SMTP_URL` (Mailpit in every environment so far, §18.5). */
export async function sendMail(msg: MailMessage): Promise<void> {
  const env = getEnv();
  transport ??= nodemailer.createTransport(env.SMTP_URL);
  await transport.sendMail({ from: env.MAIL_FROM, ...msg });
}

const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
  );

/** The magic-link email (Task 17 copy). */
export function magicLinkEmail(to: string, url: string): MailMessage {
  const line = `Use this link to sign in to ${brand.name}. It works once and expires in 10 minutes.`;
  return {
    to,
    subject: "Your sign-in link",
    text: `${line}\n\n${url}\n`,
    html: `<p>${escapeHtml(line)}</p><p><a href="${escapeHtml(url)}">Sign in to ${escapeHtml(brand.name)}</a></p><p>${escapeHtml(url)}</p>`,
  };
}
