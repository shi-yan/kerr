<template>
  <div v-if="isOpen" class="image-viewer-overlay" @click.self="close">
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
      <div class="viewer-container">
        <img ref="imageRef" :src="imageSrc" alt="Image preview" />
      </div>
      <div v-if="loading" class="viewer-status">Loading...</div>
      <div v-if="error" class="viewer-error">{{ error }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from 'vue';
// @ts-ignore - viewerjs doesn't have proper TypeScript definitions
import Viewer from 'viewerjs';
import 'viewerjs/dist/viewer.css';

const props = defineProps<{
  filePath: string;
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const imageRef = ref<HTMLImageElement | null>(null);
const imageSrc = ref('');
const loading = ref(false);
const error = ref<string | null>(null);
const fileName = ref('');

let viewer: Viewer | null = null;

const loadImage = async () => {
  loading.value = true;
  error.value = null;

  try {
    imageSrc.value = `/api/files/download?path=${encodeURIComponent(props.filePath)}`;
    fileName.value = props.filePath.split('/').pop() || '';

    // Wait for image to load
    if (imageRef.value) {
      imageRef.value.onload = () => {
        loading.value = false;
        initializeViewer();
      };
      imageRef.value.onerror = () => {
        loading.value = false;
        error.value = 'Failed to load image';
      };
    }
  } catch (e) {
    loading.value = false;
    error.value = e instanceof Error ? e.message : 'Failed to load image';
  }
};

const initializeViewer = () => {
  if (!imageRef.value) return;

  // Destroy existing viewer if any
  if (viewer) {
    viewer.destroy();
  }

  viewer = new Viewer(imageRef.value, {
    inline: false,
    viewed() {
      // Show viewer after image is loaded
    },
    navbar: false,
    title: false,
    toolbar: {
      zoomIn: true,
      zoomOut: true,
      oneToOne: true,
      reset: true,
      rotateLeft: true,
      rotateRight: true,
      flipHorizontal: true,
      flipVertical: true,
    },
  });

  viewer.show();
};

const close = () => {
  if (viewer) {
    viewer.destroy();
    viewer = null;
  }
  emit('close');
};

watch(() => props.isOpen, (newValue) => {
  if (newValue) {
    loadImage();
  } else {
    if (viewer) {
      viewer.destroy();
      viewer = null;
    }
    imageSrc.value = '';
    error.value = null;
  }
});

onBeforeUnmount(() => {
  if (viewer) {
    viewer.destroy();
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
  background: rgba(0, 0, 0, 0.9);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.image-viewer-modal {
  width: 90vw;
  height: 90vh;
  background: #1e1e1e;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5);
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
}

.viewer-container img {
  display: none;
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
/* Override viewerjs styles for dark theme */
.viewer-backdrop {
  background-color: rgba(0, 0, 0, 0.9);
}

.viewer-container {
  background-color: transparent;
}

.viewer-toolbar > ul > li {
  background-color: rgba(0, 0, 0, 0.5);
  color: #d4d4d4;
}

.viewer-toolbar > ul > li:hover {
  background-color: rgba(0, 0, 0, 0.7);
}
</style>
