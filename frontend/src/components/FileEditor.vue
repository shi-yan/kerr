<template>
  <div class="file-editor">
    <div class="editor-header">
      <span class="file-path">{{ filePath || 'Select a file to view or edit' }}</span>
    </div>

    <div class="editor-content">
      <div v-if="!filePath" class="empty-state">
        <div class="empty-icon">📝</div>
        <div class="empty-text">Select a file from the browser to view its contents</div>
      </div>

      <div v-else-if="loading" class="loading">Loading file...</div>
      <div v-else-if="error" class="error">{{ error }}</div>

      <textarea
        v-else
        v-model="content"
        class="editor"
        :readonly="true"
        spellcheck="false"
      ></textarea>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { apiClient } from '../api/client';

const props = defineProps<{
  filePath: string | null;
}>();

const content = ref('');
const loading = ref(false);
const error = ref<string | null>(null);

const loadFile = async (path: string) => {
  loading.value = true;
  error.value = null;
  content.value = '';

  try {
    const response = await apiClient.readFile(path);
    content.value = response.content;
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load file';
  } finally {
    loading.value = false;
  }
};

watch(
  () => props.filePath,
  (newPath) => {
    if (newPath) {
      loadFile(newPath);
    }
  }
);
</script>

<style scoped>
.file-editor {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #1e1e1e;
}

.editor-header {
  padding: 10px 15px;
  background: #2d2d30;
  border-bottom: 1px solid #3e3e42;
  font-size: 13px;
  color: #d4d4d4;
}

.file-path {
  color: #cccccc;
}

.editor-content {
  flex: 1;
  position: relative;
  display: flex;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: #858585;
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 15px;
}

.empty-text {
  font-size: 14px;
}

.loading,
.error {
  padding: 20px;
  text-align: center;
}

.error {
  color: #f48771;
  background: #5a1d1d;
  margin: 10px;
  border-radius: 3px;
}

.editor {
  flex: 1;
  width: 100%;
  padding: 15px;
  background: #1e1e1e;
  color: #d4d4d4;
  border: none;
  outline: none;
  font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
  font-size: 14px;
  line-height: 1.5;
  resize: none;
}

.editor::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

.editor::-webkit-scrollbar-track {
  background: #1e1e1e;
}

.editor::-webkit-scrollbar-thumb {
  background: #424242;
  border-radius: 5px;
}

.editor::-webkit-scrollbar-thumb:hover {
  background: #4e4e4e;
}
</style>
