const { deepFn } = require("./mods/cjs.js");

function topFn(num) {
  return deepFn(num);
};

module.exports = { topFn };