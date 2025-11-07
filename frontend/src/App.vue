<template>
  <div class="app">
    <ConnectionSelector v-if="!connected" @connected="handleConnected" />

    <template v-else>
      <div class="container">
        <div class="terminal-panel" :style="{ width: terminalWidth + 'px' }">
          <Terminal />
        </div>

        <div class="divider" @mousedown="startResize"></div>

        <div class="browser-panel">
          <FileBrowser :connectionString="connectionString" />
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import ConnectionSelector from './components/ConnectionSelector.vue';
import Terminal from './components/Terminal.vue';
import FileBrowser from './components/FileBrowser.vue';

const connected = ref(false);
const connectionString = ref<string>('');
const terminalWidth = ref(800); // Default width
const isResizing = ref(false);

const checkConnectionStatus = async () => {
  try {
    const response = await fetch('/api/connection/status');
    if (response.ok) {
      const data = await response.json();
      connected.value = data.connected;
      if (data.connection_string) {
        connectionString.value = data.connection_string;
      }
    }
  } catch (e) {
    console.error('Failed to check connection status:', e);
  }
};

const handleConnected = async () => {
  connected.value = true;
  // Fetch connection string after connecting
  await checkConnectionStatus();
};

const startResize = (e: MouseEvent) => {
  isResizing.value = true;
  e.preventDefault();
};

const onMouseMove = (e: MouseEvent) => {
  if (!isResizing.value) return;

  // Set terminal width based on mouse X position
  const minWidth = 300;
  const maxWidth = window.innerWidth - 300; // Leave at least 300px for file browser
  const newWidth = Math.max(minWidth, Math.min(maxWidth, e.clientX));
  terminalWidth.value = newWidth;
};

const stopResize = () => {
  isResizing.value = false;
};

onMounted(() => {
  checkConnectionStatus();

  // Initialize terminal width to 60% of window width
  terminalWidth.value = Math.floor(window.innerWidth * 0.6);

  // Add global mouse event listeners for resize
  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', stopResize);
});

onUnmounted(() => {
  // Clean up event listeners
  document.removeEventListener('mousemove', onMouseMove);
  document.removeEventListener('mouseup', stopResize);
});
</script>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
  background: #1e1e1e;
  color: #d4d4d4;
  overflow: hidden;
}

#app {
  height: 100vh;
  width: 100vw;
  display: flex;
  flex-direction: column;
}
</style>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
}

.container {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.terminal-panel {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.divider {
  width: 4px;
  background: #3e3e42;
  cursor: col-resize;
  flex-shrink: 0;
  transition: background 0.2s;
}

.divider:hover {
  background: #007acc;
}

.browser-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
