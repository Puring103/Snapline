import type { Item } from './types';

export const initialItems: Item[] = [
  {
    id: 'd104a3bc-0a55-43f5-a4aa-44cedcc2f2ec',
    content: {
      title: '关于新产品首页的想法',
      markdown: '# 首页思路\n\n不要急着解释所有功能，先让用户感受到 **捕捉的速度**。\n\n- 首屏直接进入工作台\n- 输入完成后自动归档\n- 用时间线而不是文件夹组织\n\n> 好的工具应该在想法消失之前出现。',
      source_type: 'text', tags: ['产品', '设计'], markers: ['重要'], attachment_ids: [],
      ai_metadata: { summary: '产品首页应直接呈现捕捉体验，并以时间线降低整理成本。', topics: ['产品设计'], entities: ['Snapline'], keywords: ['快速捕捉', '时间线'], search_text: '产品 首页 快速捕捉 时间线' },
    }, created_at: '2026-08-01T01:42:00.000Z', updated_at: '2026-08-01T01:42:00.000Z', version: 1, archived: false, pinned: true, sync_status: 'synced', ai_status: 'complete',
  },
  {
    id: '86077b7d-e29e-4aba-ad7f-1b65fa85e44d',
    content: { title: '散步时的语音想法', markdown: '录音已经自动转写。\n\n将零散灵感自动聚合成主题，每周生成一份可以行动的回顾。', source_type: 'audio', tags: ['AI', '工作流'], markers: [], attachment_ids: [], ai_metadata: { summary: '将零散灵感自动聚合成主题，并定期生成行动回顾。', transcript: '将零散灵感自动聚合成主题，每周生成一份可以行动的回顾。', topics: ['个人知识管理'], entities: [], keywords: ['灵感聚合', '周报'], search_text: '灵感 聚合 主题 每周 回顾' } },
    created_at: '2026-08-01T00:16:00.000Z', updated_at: '2026-08-01T00:18:00.000Z', version: 1, archived: false, pinned: false, sync_status: 'synced', ai_status: 'complete', audio_duration: '02:18',
  },
  {
    id: 'f92fbc6c-159f-48c3-be05-bbd4bca5774a',
    content: { title: '团队午餐票据', markdown: '项目讨论时的午餐票据，后续统一整理。', source_type: 'image', tags: ['项目'], markers: ['账目'], attachment_ids: [], ai_metadata: { summary: '一张团队午餐票据，标记为账目记录。', topics: ['票据'], entities: [], keywords: ['团队午餐'], search_text: '团队 午餐 票据 账目' } },
    created_at: '2026-07-31T04:34:00.000Z', updated_at: '2026-07-31T04:34:00.000Z', version: 1, archived: false, pinned: false, sync_status: 'synced', ai_status: 'complete',
    preview_image: 'https://images.unsplash.com/photo-1552566626-52f8b828add9?auto=format&fit=crop&w=720&q=80',
  },
  {
    id: '8cdb75bd-45e3-438e-bffc-bca59b3967c0',
    content: { title: '阅读器排版参考', markdown: '高密度阅读器布局：窄侧栏、清晰层级和低干扰的暖灰背景。', source_type: 'screenshot', tags: ['参考', 'UI'], markers: ['稍后处理'], attachment_ids: [], ai_metadata: { summary: '高密度阅读器布局参考。', topics: ['界面设计'], entities: [], keywords: ['排版', '阅读器'], search_text: '阅读器 排版 高密度 界面' } },
    created_at: '2026-07-31T02:05:00.000Z', updated_at: '2026-07-31T02:05:00.000Z', version: 1, archived: false, pinned: false, sync_status: 'synced', ai_status: 'complete',
    preview_image: 'https://images.unsplash.com/photo-1499750310107-5fef28a66643?auto=format&fit=crop&w=720&q=80',
  },
];
