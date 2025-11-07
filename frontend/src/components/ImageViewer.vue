<template>
  <div v-if="isOpen" class="image-viewer-overlay">
    <div class="image-viewer-modal">
      <div class="viewer-header">
        <div class="header-left">
          <span class="material-symbols-outlined">image</span>
          <span class="file-name">{{ fileName }}</span>
        </div>
        <button class="icon-btn" @click="close" title="Close">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div ref="viewerContainer" class="viewer-container">
        <img v-if="imageSrc" :src="imageSrc" alt="Image preview" @click="showViewer" />
      </div>
      <div v-if="loading" class="viewer-status">Loading...</div>
      <div v-if="error" class="viewer-error">{{ error }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue';
import { api as viewerApi } from 'v-viewer';
import 'viewerjs/dist/viewer.css';

const props = defineProps<{
  filePath: string;
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const viewerContainer = ref<HTMLElement | null>(null);
const imageSrc = ref('');
const loading = ref(false);
const error = ref<string | null>(null);
const fileName = ref('');

const showViewer = () => {
  if (!imageSrc.value) return;

  viewerApi({
    images: [imageSrc.value],
    options: {
      toolbar: {
        zoomIn: 4,
        zoomOut: 4,
        oneToOne: 4,
        reset: 4,
        prev: false,
        play: false,
        next: false,
        rotateLeft: 4,
        rotateRight: 4,
        flipHorizontal: 4,
        flipVertical: 4,
      },
      tooltip: true,
      movable: true,
      zoomable: true,
      rotatable: true,
      scalable: true,
      transition: true,
      fullscreen: true,
      keyboard: true,
      title: false,
      navbar: false,
      backdrop: true,
      url: 'src',
    },
  });
};

const loadImage = async () => {
  loading.value = true;
  error.value = null;

  try {
    imageSrc.value = `/api/files/download?path=${encodeURIComponent(props.filePath)}`;
    fileName.value = props.filePath.split('/').pop() || '';

    await nextTick();
    loading.value = false;

    // Auto-show viewer after image loads
    await nextTick();
    showViewer();
  } catch (e) {
    loading.value = false;
    error.value = e instanceof Error ? e.message : 'Failed to load image';
  }
};

const close = () => {
  emit('close');
};

watch(() => props.isOpen, (newValue) => {
  if (newValue) {
    loadImage();
  } else {
    imageSrc.value = '';
    error.value = null;
  }
});
</script>

<style scoped>
.image-viewer-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: #1e1e1e;
  z-index: 1000;
}

.image-viewer-modal {
  width: 100%;
  height: 100%;
  background: #1e1e1e;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.viewer-header {
  padding: 12px 16px;
  background: #2d2d30;
  border-bottom: 1px solid #3e3e42;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #d4d4d4;
}

.header-left .material-symbols-outlined {
  font-size: 20px;
}

.file-name {
  font-size: 14px;
  font-weight: 500;
}

.icon-btn {
  padding: 6px;
  background: transparent;
  color: #d4d4d4;
  border: none;
  border-radius: 3px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.2s;
}

.icon-btn:hover {
  background: #3e3e42;
}

.icon-btn .material-symbols-outlined {
  font-size: 20px;
}

.viewer-container {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  background: #252526;
  position: relative;
}

.viewer-container img {
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
  cursor: pointer;
  display: block;
}

.viewer-status {
  padding: 8px 16px;
  background: #2d2d30;
  border-top: 1px solid #3e3e42;
  color: #4fc3f7;
  font-size: 12px;
}

.viewer-error {
  padding: 8px 16px;
  background: #5a1d1d;
  border-top: 1px solid #3e3e42;
  color: #f48771;
  font-size: 12px;
}
</style>

<style>
/* Override viewerjs styles for dark theme and inline mode */
.viewer-container .viewer-container {
  background-color: #252526;
}

.viewer-canvas {
  background-color: transparent;
}

.viewer-toolbar > ul > li {
  background-color: rgba(45, 45, 48, 0.9);
  color: #d4d4d4;
  border-radius: 4px;
}

.viewer-toolbar > ul > li:hover {
  background-color: rgba(62, 62, 66, 0.9);
}

.viewer-button {
  background-color: transparent;
  color: #d4d4d4;
}
</style>
