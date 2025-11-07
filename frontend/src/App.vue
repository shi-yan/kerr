<template>
  <div class="app">
    <ConnectionSelector v-if="!connected" @connected="handleConnected" />

    <template v-else>
      <div class="container">
        <div class="terminal-panel">
          <Terminal />
        </div>

        <div class="browser-panel">
          <FileBrowser />
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import ConnectionSelector from './components/ConnectionSelector.vue';
import Terminal from './components/Terminal.vue';
import FileBrowser from './components/FileBrowser.vue';

const connected = ref(false);

const checkConnectionStatus = async () => {
  try {
    const response = await fetch('/api/connection/status');
    if (response.ok) {
      const data = await response.json();
      connected.value = data.connected;
    }
  } catch (e) {
    console.error('Failed to check connection status:', e);
  }
};

const handleConnected = () => {
  connected.value = true;
};

onMounted(() => {
  checkConnectionStatus();
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
  flex: 1;
  display: flex;
  flex-direction: column;
  border-right: 1px solid #3e3e42;
}

.browser-panel {
  width: 400px;
  display: flex;
  flex-direction: column;
}
</style>
