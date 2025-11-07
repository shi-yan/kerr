<template>
  <div v-if="isOpen" class="file-editor-overlay" @click.self="close">
    <div class="file-editor-modal">
      <div class="editor-header">
        <div class="header-left">
          <span class="material-symbols-outlined">description</span>
          <span class="file-name">{{ fileName }}</span>
        </div>
        <div class="header-actions">
          <button class="icon-btn" @click="saveFile" :disabled="!isDirty || saving" title="Save (Ctrl+S)">
            <span class="material-symbols-outlined">save</span>
          </button>
          <button class="icon-btn" @click="close" title="Close">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div ref="editorContainer" class="editor-container"></div>
      <div v-if="saving" class="editor-status">Saving...</div>
      <div v-if="saveError" class="editor-error">{{ saveError }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onBeforeUnmount, nextTick } from 'vue';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState } from '@codemirror/state';
import { javascript } from '@codemirror/lang-javascript';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { python } from '@codemirror/lang-python';
import { oneDark } from '@codemirror/theme-one-dark';
import { keymap, ViewUpdate } from '@codemirror/view';

const props = defineProps<{
  filePath: string;
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'saved'): void;
}>();

const editorContainer = ref<HTMLElement | null>(null);
const isDirty = ref(false);
const saving = ref(false);
const saveError = ref<string | null>(null);
const fileName = ref('');
const fileContent = ref('');

let editorView: EditorView | null = null;

const getLanguageExtension = (filePath: string) => {
  const ext = filePath.split('.').pop()?.toLowerCase();
  switch (ext) {
    case 'js':
    case 'jsx':
    case 'ts':
    case 'tsx':
      return javascript();
    case 'html':
    case 'htm':
      return html();
    case 'css':
    case 'scss':
    case 'less':
      return css();
    case 'json':
      return json();
    case 'md':
    case 'markdown':
      return markdown();
    case 'py':
      return python();
    default:
      return [];
  }
};

const loadFile = async () => {
  try {
    const response = await fetch(`/api/file/content?path=${encodeURIComponent(props.filePath)}`);
    if (!response.ok) {
      throw new Error('Failed to load file');
    }
    const data = await response.json();
    fileContent.value = data.content;
    fileName.value = props.filePath.split('/').pop() || '';
    initializeEditor();
  } catch (e) {
    saveError.value = e instanceof Error ? e.message : 'Failed to load file';
  }
};

const initializeEditor = async () => {
  await nextTick();
  if (!editorContainer.value) return;

  // Dispose existing editor if any
  if (editorView) {
    editorView.destroy();
  }

  const languageExtension = getLanguageExtension(props.filePath);

  const state = EditorState.create({
    doc: fileContent.value,
    extensions: [
      basicSetup,
      languageExtension,
      oneDark,
      keymap.of([
        {
          key: 'Mod-s',
          run: () => {
            saveFile();
            return true;
          },
        },
      ]),
      EditorView.updateListener.of((update: ViewUpdate) => {
        if (update.docChanged) {
          isDirty.value = true;
        }
      }),
    ],
  });

  editorView = new EditorView({
    state,
    parent: editorContainer.value,
  });
};

const saveFile = async () => {
  if (!editorView || saving.value) return;

  saving.value = true;
  saveError.value = null;

  try {
    const content = editorView.state.doc.toString();
    const response = await fetch('/api/file/content', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        path: props.filePath,
        content,
      }),
    });

    if (!response.ok) {
      throw new Error('Failed to save file');
    }

    isDirty.value = false;
    emit('saved');
  } catch (e) {
    saveError.value = e instanceof Error ? e.message : 'Failed to save file';
  } finally {
    saving.value = false;
  }
};

const close = () => {
  if (isDirty.value) {
    if (!confirm('You have unsaved changes. Are you sure you want to close?')) {
      return;
    }
  }
  emit('close');
};

watch(() => props.isOpen, (newValue) => {
  if (newValue) {
    loadFile();
  } else {
    if (editorView) {
      editorView.destroy();
      editorView = null;
    }
    isDirty.value = false;
    saveError.value = null;
  }
});

onBeforeUnmount(() => {
  if (editorView) {
    editorView.destroy();
  }
});
</script>

<style scoped>
.file-editor-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.8);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.file-editor-modal {
  width: 90vw;
  height: 90vh;
  background: #282c34;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5);
}

.editor-header {
  padding: 12px 16px;
  background: #21252b;
  border-bottom: 1px solid #181a1f;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #abb2bf;
}

.header-left .material-symbols-outlined {
  font-size: 20px;
}

.file-name {
  font-size: 14px;
  font-weight: 500;
}

.header-actions {
  display: flex;
  gap: 8px;
}

.icon-btn {
  padding: 6px;
  background: transparent;
  color: #abb2bf;
  border: none;
  border-radius: 3px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.2s;
}

.icon-btn:hover:not(:disabled) {
  background: #2c313c;
}

.icon-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.icon-btn .material-symbols-outlined {
  font-size: 20px;
}

.editor-container {
  flex: 1;
  overflow: auto;
}

.editor-container :deep(.cm-editor) {
  height: 100%;
  font-size: 14px;
}

.editor-container :deep(.cm-scroller) {
  overflow: auto;
}

.editor-status {
  padding: 8px 16px;
  background: #21252b;
  border-top: 1px solid #181a1f;
  color: #61afef;
  font-size: 12px;
}

.editor-error {
  padding: 8px 16px;
  background: #5a1d1d;
  border-top: 1px solid #181a1f;
  color: #f48771;
  font-size: 12px;
}
</style>
