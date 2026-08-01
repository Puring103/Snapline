export type SourceType = 'text' | 'screenshot' | 'image' | 'audio' | 'video' | 'mixed';
export type AiStatus = 'unconfigured' | 'pending' | 'processing' | 'complete' | 'failed';

export interface ItemContent {
  title: string;
  markdown: string;
  source_type: SourceType;
  tags: string[];
  markers: string[];
  attachment_ids: string[];
  ai_metadata: null | {
    summary: string;
    transcript?: string;
    topics: string[];
    entities: string[];
    keywords: string[];
    search_text: string;
  };
}

export interface Item {
  id: string;
  content: ItemContent;
  created_at: string;
  updated_at: string;
  version: number;
  archived: boolean;
  pinned: boolean;
  sync_status: string;
  ai_status?: AiStatus;
  preview_image?: string;
  audio_duration?: string;
}

export type View = 'all' | 'pinned' | 'archive' | `marker:${string}` | `tag:${string}`;
