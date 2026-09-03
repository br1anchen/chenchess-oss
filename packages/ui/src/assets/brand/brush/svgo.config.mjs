export default {
  multipass: true,
  floatPrecision: 0,
  plugins: [
    { name: "preset-default", params: { overrides: { removeViewBox: false } } },
    "removeDimensions",
    {
      name: "convertPathData",
      params: {
        floatPrecision: 0,
        transformPrecision: 0,
        applyTransforms: true,
        makeArcs: { threshold: 4, tolerance: 1 },
      },
    },
    "mergePaths",
    { name: "cleanupNumericValues", params: { floatPrecision: 0 } },
  ],
}
