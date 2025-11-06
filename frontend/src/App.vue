<template>
  <div class="app">
    <header class="header">
      <h1 class="title">🌌 Kerr Remote File Browser</h1>
    </header>

    <div class="container">
      <div class="browser-panel">
        <FileBrowser @file-selected="handleFileSelected" />
      </div>

      <div class="editor-panel">
        <FileEditor :file-path="selectedFilePath" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import FileBrowser from './components/FileBrowser.vue';
import FileEditor from './components/FileEditor.vue';

const selectedFilePath = ref<string | null>(null);

const handleFileSelected = (path: string) => {
  selectedFilePath.value = path;
};
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

.header {
  background: #252526;
  padding: 15px 20px;
  border-bottom: 1px solid #3e3e42;
}

.title {
  font-size: 18px;
  font-weight: 500;
  color: #d4d4d4;
}

.container {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.browser-panel {
  width: 350px;
  border-right: 1px solid #3e3e42;
  display: flex;
  flex-direction: column;
}

.editor-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
}
</style>
