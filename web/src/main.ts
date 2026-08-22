import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import { VueQueryPlugin } from '@tanstack/vue-query'
import '@fontsource-variable/inter'
import './style.css'
import App from './App.vue'
import { router } from './router'
import { queryClient } from './query-client'

const app = createApp(App)

app.use(PrimeVue, { unstyled: true })
app.use(VueQueryPlugin, { queryClient })
app.use(router)

// Wait for the initial navigation (including the auth guard's `me` check)
// to resolve before mounting, so the app never flashes unauthenticated
// content for a protected route.
router.isReady().then(() => {
  app.mount('#app')
})
