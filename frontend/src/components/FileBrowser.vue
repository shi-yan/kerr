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

    <div class="file-list" v-if="!loading && !error">
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
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-if="error" class="error">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { apiClient } from '../api/client';
import type { FileEntry } from '../types/api';

const props = defineProps<{
  initialPath?: string;
}>();

const emit = defineEmits<{
  fileSelected: [path: string];
}>();

const currentPath = ref('/');
const entries = ref<FileEntry[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const selectedPath = ref<string | null>(null);

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
    emit('fileSelected', entry.path);
  }
};

const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
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

.file-list {
  flex: 1;
  overflow-y: auto;
  padding: 5px;
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
