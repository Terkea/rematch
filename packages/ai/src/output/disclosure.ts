export const AI_DISCLOSURE = {
  label: "Written by AI",
  note: "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.",
} as const;

export type AiDisclosure = {
  label: typeof AI_DISCLOSURE.label;
  note: typeof AI_DISCLOSURE.note;
  model: string;
  promptVersion: string;
  generatedAt: string;
};

export function makeDisclosure(
  model: string,
  promptVersion: string,
  generatedAt: Date,
): AiDisclosure {
  return {
    ...AI_DISCLOSURE,
    model,
    promptVersion,
    generatedAt: generatedAt.toISOString(),
  };
}
