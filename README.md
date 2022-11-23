# Neko Dumper
![GitHub release (latest by date)](https://img.shields.io/github/v/release/RuyerZ/nekodumper)
![GitHub all releases](https://img.shields.io/github/downloads/RuyerZ/nekodumper/total)
![GitHub](https://img.shields.io/github/license/RuyerZ/nekodumper)

[下载地址](https://github.com/RuyerZ/nekodumper/releases/latest/)

某APP的缓存导出工具，用于备份内容。

本项目仅用于备份下架内容及在其他设备上浏览内容，**请勿无授权传播**。

**闷声大发财**，为了该工具的使用寿命，使用请低调。

## 前置需求
该工具需要将相关文件传送到电脑上，需要安卓设备的root权限。

## 用法

1. 将app文件夹中的内容传输到电脑中。
    - 钛备份：备份APP之后，将`/sdcard/TitaniumBackup`下的APP的.tar.gz文件传输到电脑中，在电脑中解压，选择`/data/data/com.xxx`文件夹。
    - 直接复制：用root文件管理器打开`/data/data/com.xxx`，将整个文件夹传输到电脑中。
  
2. 将*NekoDumper*可执行文件复制到第一步的文件夹中。
   
3. 运行*NekoDumper*。
   
4. 运行完成后文件夹中会出现备份的文件。

## 注意事项
- 如果切换多个账号，请在备份发送到电脑之前在APP中加载所有要导出的内容的章节目录。
- 章节目录必须在点击目录并翻阅一页内容之后才能加载到数据库中。

## 命令行选项
- `-n (--name) <NAME>` : 仅提取ID为`<NAME>`或书名包含`<NAME>`的书籍。
- `-e (--epub)` : 实验性功能，生成EPUB文件。需要用到网络爬虫爬取图片，爬虫不发送任何能识别用户的数据。
- `-d (--debug)` : 输出调试信息。
- `-r` : 使用Windows风格换行（\r\n），用于应对Windows上部分老旧软件无法正常处理\n换行的问题。

## 编译
1. 参考Rust圣经的[寻找牛刀，以便小试](https://course.rs/first-try/intro.html)章节，安装rustup与cargo。

2. 将代码下载或`git clone`到本地，命令行执行`cargo build`编译。
