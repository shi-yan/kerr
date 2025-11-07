<template>
  <div class="file-browser">
    <FileEditor :filePath="editorFilePath" :isOpen="isEditorOpen" @close="closeEditor" @saved="handleFileSaved" />
    <ImageViewer :filePath="imageFilePath" :isOpen="isImageViewerOpen" @close="closeImageViewer" />

    <div class="breadcrumb">
      <div class="breadcrumb-path">
        <span class="breadcrumb-item" @click="navigateTo('/')">
          <span class="material-symbols-outlined">home</span>
        </span>
        <template v-for="(part, index) in pathParts" :key="index">
          <span class="separator">/</span>
          <span class="breadcrumb-item" @click="navigateTo(getPathUpTo(index))">
            {{ part }}
          </span>
        </template>
      </div>

      <div class="breadcrumb-actions">
        <button
          class="icon-btn"
          :disabled="!selectedPath"
          @click="handleDownload"
          title="Download selected file"
        >
          <span class="material-symbols-outlined">download</span>
        </button>
        <button
          class="icon-btn btn-danger"
          :disabled="!selectedPath"
          @click="handleDelete"
          title="Delete selected file/folder"
        >
          <span class="material-symbols-outlined">delete</span>
        </button>
      </div>
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
        <span class="material-symbols-outlined icon">folder</span>
        <span class="name">..</span>
      </div>

      <div
        v-for="entry in sortedEntries"
        :key="entry.path"
        class="file-item"
        :class="{ selected: selectedPath === entry.path }"
        @click="handleItemClick(entry)"
        @dblclick="handleItemDoubleClick(entry)"
      >
        <span class="material-symbols-outlined icon">{{ entry.is_dir ? 'folder' : 'description' }}</span>
        <span class="name">{{ entry.name }}</span>
        <span class="size" v-if="!entry.is_dir">{{ formatSize(entry.size) }}</span>
      </div>

      <div v-if="isDraggingOver" class="drop-overlay">
        <div class="drop-message">
          <span class="material-symbols-outlined">upload</span>
          Drop files here to upload to {{ currentPath }}
        </div>
      </div>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-if="error" class="error">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { apiClient } from '../api/client';
import type { FileEntry } from '../types/api';
import FileEditor from './FileEditor.vue';
import ImageViewer from './ImageViewer.vue';

const props = defineProps<{
  initialPath?: string;
  connectionString?: string;
}>();

const currentPath = ref('/');
const entries = ref<FileEntry[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const selectedPath = ref<string | null>(null);
const isDraggingOver = ref(false);
const uploadingFiles = ref<Set<string>>(new Set());

// File editor and image viewer state
const isEditorOpen = ref(false);
const editorFilePath = ref('');
const isImageViewerOpen = ref(false);
const imageFilePath = ref('');

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

// File type detection
const isTextFile = (filename: string): boolean => {
  const ext = filename.split('.').pop()?.toLowerCase();
  const textExts = ['txt', 'js', 'jsx', 'ts', 'tsx', 'html', 'css', 'scss', 'less',
    'json', 'xml', 'md', 'markdown', 'py', 'rb', 'java', 'c', 'cpp', 'h', 'hpp',
    'rs', 'go', 'php', 'sh', 'bash', 'yml', 'yaml', 'toml', 'ini', 'conf', 'log'];
  return ext ? textExts.includes(ext) : false;
};

const isImageFile = (filename: string): boolean => {
  const ext = filename.split('.').pop()?.toLowerCase();
  const imageExts = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg'];
  return ext ? imageExts.includes(ext) : false;
};

// Handle double-click to open files
const handleItemDoubleClick = (entry: FileEntry) => {
  if (entry.is_dir) {
    // Navigate into directory
    navigateTo(entry.path);
  } else {
    // Open file based on type
    if (isTextFile(entry.name)) {
      editorFilePath.value = entry.path;
      isEditorOpen.value = true;
    } else if (isImageFile(entry.name)) {
      imageFilePath.value = entry.path;
      isImageViewerOpen.value = true;
    }
  }
};

const closeEditor = () => {
  isEditorOpen.value = false;
  editorFilePath.value = '';
};

const closeImageViewer = () => {
  isImageViewerOpen.value = false;
  imageFilePath.value = '';
};

const handleFileSaved = () => {
  // Reload directory to reflect any changes
  loadDirectory(currentPath.value);
};

// LocalStorage path persistence
const STORAGE_KEY = 'kerr_file_browser_paths';

const savePathToStorage = () => {
  if (!props.connectionString) return;

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    const paths = stored ? JSON.parse(stored) : {};
    paths[props.connectionString] = currentPath.value;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(paths));
  } catch (e) {
    console.error('Failed to save path to localStorage:', e);
  }
};

const loadPathFromStorage = (): string | null => {
  if (!props.connectionString) return null;

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const paths = JSON.parse(stored);
      return paths[props.connectionString] || null;
    }
  } catch (e) {
    console.error('Failed to load path from localStorage:', e);
  }
  return null;
};

// Flag to track if we've done initial load
const hasLoadedInitially = ref(false);

// Watch for path changes to save to localStorage
watch(currentPath, () => {
  savePathToStorage();
});

// Watch for connectionString to become available and load saved path
watch(() => props.connectionString, async (newConnectionString, oldConnectionString) => {
  // Only proceed if connectionString changed from empty to non-empty (initial connection)
  if (newConnectionString && !oldConnectionString && !hasLoadedInitially.value) {
    hasLoadedInitially.value = true;

    // Load saved path for this connection, or default to root
    const savedPath = loadPathFromStorage();
    const initialPath = savedPath || props.initialPath || '/';

    console.log('Loading initial path:', initialPath, 'for connection:', newConnectionString.substring(0, 10));
    await loadDirectory(initialPath);
  }
});

onMounted(async () => {
  // If connectionString is already available (unlikely but possible), load immediately
  if (props.connectionString && !hasLoadedInitially.value) {
    hasLoadedInitially.value = true;
    const savedPath = loadPathFromStorage();
    const initialPath = savedPath || props.initialPath || '/';
    await loadDirectory(initialPath);
  } else if (!props.connectionString) {
    // If no connection string yet, load root as placeholder
    await loadDirectory('/');
  }
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
  justify-content: space-between;
  gap: 15px;
}

.breadcrumb-path {
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
  flex: 1;
  min-width: 0;
}

.breadcrumb-item {
  cursor: pointer;
  color: #4fc3f7;
  user-select: none;
  display: flex;
  align-items: center;
  gap: 3px;
}

.breadcrumb-item:hover {
  text-decoration: underline;
}

.breadcrumb-item .material-symbols-outlined {
  font-size: 18px;
}

.separator {
  color: #858585;
}

.breadcrumb-actions {
  display: flex;
  gap: 5px;
  flex-shrink: 0;
}

.icon-btn {
  padding: 6px;
  background: transparent;
  color: #d4d4d4;
  border: 1px solid #3e3e42;
  border-radius: 3px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
  width: 32px;
  height: 32px;
}

.icon-btn:hover:not(:disabled) {
  background: #0e639c;
  border-color: #0e639c;
  color: white;
}

.icon-btn:disabled {
  background: transparent;
  border-color: #2d2d30;
  color: #3e3e42;
  cursor: not-allowed;
}

.icon-btn.btn-danger:hover:not(:disabled) {
  background: #c72e0f;
  border-color: #c72e0f;
  color: white;
}

.icon-btn .material-symbols-outlined {
  font-size: 20px;
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
  display: flex;
  align-items: center;
  gap: 10px;
}

.drop-message .material-symbols-outlined {
  font-size: 32px;
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
  font-size: 18px;
  color: #d4d4d4;
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
