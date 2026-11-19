import java.nio.file.Files;
import java.nio.file.Path;

public final class FwBridge {
    public static void main(String[] args) {
        String root = args.length > 0 ? args[0] : ".";
        Path ghidra = Path.of(root, "packages", "ghidra");
        boolean ghidraPresent = Files.isDirectory(ghidra);
        String json = String.format(
            "{\"ok\":true,\"product\":\"fw\",\"lang\":\"java\",\"javaVersion\":\"%s\",\"ghidraPresent\":%s,\"ghidraPath\":\"%s\"}",
            System.getProperty("java.version"),
            ghidraPresent,
            ghidra.toAbsolutePath().normalize().toString().replace("\\", "\\\\")
        );
        System.out.println(json);
    }
}
