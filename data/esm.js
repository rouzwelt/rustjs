import { deepFn } from "./mods/esm.js";

function topFn(num) {
  return deepFn(num);
}

function fnWithError() {
  throw new Error("some error")
}

async function asyncFnResolve(num) {
  await sleep(1000);
  return Promise.resolve(num);
}

async function asyncFnReject() {
  await sleep(1000);
  return Promise.reject(new Error("rejected"));
}

async function sleep(ms, msg = "") {
  let _timeoutReference;
  return new Promise(
      resolve => _timeoutReference = setTimeout(() => resolve(msg), ms),
  ).finally(
      () => clearTimeout(_timeoutReference)
  );
};

export { topFn, asyncFnResolve, asyncFnReject, fnWithError };
