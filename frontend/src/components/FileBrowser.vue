<template>
  <div class="file-browser">
    <div class="breadcrumb">
      <span class="breadcrumb-item" @click="navigateTo('/')">📁 /</span>
      <template v-for="(part, index) in pathParts" :key="index">
        <span class="separator">/</span>
        <span class="breadcrumb-item" @click="navigateTo(getPathUpTo(index))">
          {{ part }}
        </span>
      </template>
    </div>

    <div class="toolbar">
      <button
        class="btn"
        :disabled="!selectedPath"
        @click="handleDownload"
        title="Download selected file"
      >
        ⬇ Download
      </button>
      <button
        class="btn btn-danger"
        :disabled="!selectedPath"
        @click="handleDelete"
        title="Delete selected file/folder"
      >
        🗑 Delete
      </button>
    </div>

    <div
      class="file-list"
      v-if="!loading && !error"
      @dragover.prevent="handleDragOver"
      @dragleave.prevent="handleDragLeave"
      @drop.prevent="handleDrop"
      :class="{ 'drag-over': isDraggingOver }"
    >
      <div
        v-if="currentPath !== '/'"
        class="file-item"
        @click="navigateUp"
      >
        <span class="icon">📁</span>
        <span class="name">..</span>
      </div>

      <div
        v-for="entry in sortedEntries"
        :key="entry.path"
        class="file-item"
        :class="{ selected: selectedPath === entry.path }"
        @click="handleItemClick(entry)"
      >
        <span class="icon">{{ entry.is_dir ? '📁' : '📄' }}</span>
        <span class="name">{{ entry.name }}</span>
        <span class="size" v-if="!entry.is_dir">{{ formatSize(entry.size) }}</span>
      </div>

      <div v-if="isDraggingOver" class="drop-overlay">
        <div class="drop-message">
          📤 Drop files here to upload to {{ currentPath }}
        </div>
      </div>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-if="error" class="error">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { apiClient } from '../api/client';
import type { FileEntry } from '../types/api';

const props = defineProps<{
  initialPath?: string;
}>();

const currentPath = ref('/');
const entries = ref<FileEntry[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const selectedPath = ref<string | null>(null);
const isDraggingOver = ref(false);
const uploadingFiles = ref<Set<string>>(new Set());

const pathParts = computed(() => {
  return currentPath.value
    .split('/')
    .filter(p => p.length > 0);
});

const sortedEntries = computed(() => {
  return [...entries.value].sort((a, b) => {
    // Directories first
    if (a.is_dir && !b.is_dir) return -1;
    if (!a.is_dir && b.is_dir) return 1;
    // Then alphabetically
    return a.name.localeCompare(b.name);
  });
});

const loadDirectory = async (path: string) => {
  loading.value = true;
  error.value = null;

  try {
    const response = await apiClient.listFiles(path);
    entries.value = response.entries;
    currentPath.value = path;
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load directory';
  } finally {
    loading.value = false;
  }
};

const navigateTo = (path: string) => {
  loadDirectory(path);
};

const navigateUp = () => {
  const parts = currentPath.value.split('/').filter(p => p.length > 0);
  parts.pop();
  const newPath = parts.length > 0 ? '/' + parts.join('/') : '/';
  navigateTo(newPath);
};

const getPathUpTo = (index: number): string => {
  const parts = pathParts.value.slice(0, index + 1);
  return '/' + parts.join('/');
};

const handleItemClick = (entry: FileEntry) => {
  if (entry.is_dir) {
    navigateTo(entry.path);
  } else {
    selectedPath.value = entry.path;
  }
};

const handleDownload = async () => {
  if (!selectedPath.value) return;

  try {
    // Create a temporary anchor element to trigger download
    const url = `/api/files/download?path=${encodeURIComponent(selectedPath.value)}`;
    const link = document.createElement('a');
    link.href = url;
    link.download = selectedPath.value.split('/').pop() || 'download';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to download file';
  }
};

const handleDelete = async () => {
  if (!selectedPath.value) return;

  const confirmed = confirm(`Are you sure you want to delete:\n${selectedPath.value}`);
  if (!confirmed) return;

  try {
    await apiClient.deleteFile(selectedPath.value);
    selectedPath.value = null;
    // Reload current directory
    await loadDirectory(currentPath.value);
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to delete file';
  }
};

const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
};

const handleDragOver = (e: DragEvent) => {
  if (e.dataTransfer) {
    e.dataTransfer.dropEffect = 'copy';
  }
  isDraggingOver.value = true;
};

const handleDragLeave = () => {
  isDraggingOver.value = false;
};

const handleDrop = async (e: DragEvent) => {
  isDraggingOver.value = false;

  if (!e.dataTransfer?.files || e.dataTransfer.files.length === 0) {
    return;
  }

  const files = Array.from(e.dataTransfer.files);

  for (const file of files) {
    await uploadFile(file);
  }
};

const uploadFile = async (file: File) => {
  const fileName = file.name;
  const targetPath = currentPath.value === '/'
    ? `/${fileName}`
    : `${currentPath.value}/${fileName}`;

  uploadingFiles.value.add(fileName);

  try {
    const formData = new FormData();
    formData.append('file', file);
    formData.append('path', targetPath);

    const response = await fetch('/api/files/upload', {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(errorText || 'Upload failed');
    }

    // Reload directory after successful upload
    await loadDirectory(currentPath.value);
  } catch (e) {
    error.value = e instanceof Error ? e.message : `Failed to upload ${fileName}`;
  } finally {
    uploadingFiles.value.delete(fileName);
  }
};

onMounted(() => {
  loadDirectory(props.initialPath || '/');
});
</script>

<style scoped>
.file-browser {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #252526;
}

.breadcrumb {
  padding: 10px 15px;
  background: #2d2d30;
  border-bottom: 1px solid #3e3e42;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
}

.breadcrumb-item {
  cursor: pointer;
  color: #4fc3f7;
  user-select: none;
}

.breadcrumb-item:hover {
  text-decoration: underline;
}

.separator {
  color: #858585;
}

.toolbar {
  padding: 8px 15px;
  background: #2d2d30;
  border-bottom: 1px solid #3e3e42;
  display: flex;
  gap: 10px;
}

.btn {
  padding: 6px 12px;
  background: #0e639c;
  color: white;
  border: none;
  border-radius: 3px;
  cursor: pointer;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 5px;
  transition: background 0.2s;
}

.btn:hover:not(:disabled) {
  background: #1177bb;
}

.btn:disabled {
  background: #3e3e42;
  color: #858585;
  cursor: not-allowed;
}

.btn-danger {
  background: #c72e0f;
}

.btn-danger:hover:not(:disabled) {
  background: #e81123;
}

.file-list {
  flex: 1;
  overflow-y: auto;
  padding: 5px;
  position: relative;
}

.file-list.drag-over {
  background: rgba(14, 99, 156, 0.1);
  border: 2px dashed #007acc;
}

.drop-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 122, 204, 0.2);
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
  z-index: 10;
}

.drop-message {
  background: #007acc;
  color: white;
  padding: 20px 40px;
  border-radius: 8px;
  font-size: 16px;
  font-weight: 500;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.file-item {
  padding: 8px 15px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
  border-radius: 3px;
  font-size: 13px;
  user-select: none;
}

.file-item:hover {
  background: #2a2d2e;
}

.file-item.selected {
  background: #094771;
}

.icon {
  width: 20px;
  flex-shrink: 0;
}

.name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.size {
  color: #858585;
  font-size: 12px;
  flex-shrink: 0;
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
</style>
