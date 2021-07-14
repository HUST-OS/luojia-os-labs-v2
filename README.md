# 洛佳的异步内核实验室

## 运行内核

使用以下指令来运行内核：

```bash
cargo qemu
```

编译并打包多个程序，再运行内核。以hello-world为例：

```bash
cargo qemu hello-world
```

指令`cargo qemu`可以添加`--release`参数。

## 内核程序联合调试

使用以下指令：

```bash
# 打开一个窗口
cargo debug hello-world
# 打开另一个窗口
cargo gdb
```

调试指令不能添加`--release`参数。
