// SLICE_014 §4: a tiny, deliberately separate module so the router's Today
// route record and LoginView's `onMounted` both trigger the SAME dynamic
// import — the second call resolves instantly from Vite's module cache
// rather than starting a second fetch — and so a test can spy on this one
// export instead of mocking the router or intercepting `import()`.
export const preloadTodayView = () => import('./views/TodayView.vue')
