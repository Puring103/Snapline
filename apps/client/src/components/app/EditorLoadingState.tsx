import { hasTransientImageSource } from "../../features/editor/markdown";

export function EditorLoadingState({ bodyMarkdown }: { bodyMarkdown: string }) {
  return (
    <div className="editorShell">
      <textarea
        aria-label="Note body loading"
        className="editorSurface editorLoadingSurface"
        readOnly
        value={bodyMarkdown}
      />
      {hasTransientImageSource(bodyMarkdown) ? (
        <div className="editorHint">Uploading image...</div>
      ) : null}
    </div>
  );
}
