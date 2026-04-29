# Snapline Icon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable SVG icon for Snapline and align the in-app logo with it.

**Architecture:** Keep the product icon as a standalone SVG asset under the Tauri icon folder. Keep the React header mark inline so it continues to inherit theme colors from existing CSS.

**Tech Stack:** SVG, React, TypeScript, Vite.

---

### Task 1: Add Product SVG Asset

**Files:**
- Create: `apps/desktop-tauri/src-tauri/icons/snapline.svg`

- [ ] **Step 1: Create the SVG**

Use a 1024 by 1024 viewBox with a rounded square background, warm note panel, folded corner, and fast horizontal lines.

- [ ] **Step 2: Validate XML**

Run: `node -e "const fs=require('fs'); const text=fs.readFileSync('apps/desktop-tauri/src-tauri/icons/snapline.svg','utf8'); if(!text.includes('<svg')||!text.includes('</svg>')) process.exit(1)"`

Expected: exit code 0.

### Task 2: Align In-App Logo

**Files:**
- Modify: `apps/desktop-tauri/src/App.tsx`

- [ ] **Step 1: Replace `LogoIcon` paths**

Update the inline 32 by 32 logo to use the same folded-note and fast-line silhouette.

- [ ] **Step 2: Build the frontend**

Run: `npm run build` from `apps/desktop-tauri`.

Expected: TypeScript and Vite build complete successfully.
