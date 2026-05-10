import { hasTransientImageSource } from "./markdown";

export function shouldApplyEditorMarkdownUpdate(
  currentMarkdown: string,
  nextMarkdown: string,
): boolean {
  if (currentMarkdown === nextMarkdown) {
    return false;
  }

  if (hasTransientImageSource(nextMarkdown) && !hasTransientImageSource(currentMarkdown)) {
    return false;
  }

  return true;
}
