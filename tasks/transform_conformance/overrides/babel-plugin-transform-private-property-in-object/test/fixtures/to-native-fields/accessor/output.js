var _Foo_brand = /* @__PURE__ */ new WeakSet();
class Foo {
  constructor() {
    babelHelpers.classPrivateMethodInitSpec(this, _Foo_brand);
  }
  test(other) {
    return _Foo_brand.has(babelHelpers.checkInRHS(other));
  }
}
function _get_foo() {}
