import { createApp } from 'vue'
import App from './App.vue'
import 'viewerjs/dist/viewer.css'
import VueViewer from 'v-viewer'

const app = createApp(App)
app.use(VueViewer)
app.mount('#app')
