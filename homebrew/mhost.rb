class Mhost < Formula
  desc "Advanced process manager — PM2 replacement written in Rust"
  homepage "https://github.com/maqalaqil/mhost"
  license "MIT"

  version "0.1.0"

  on_macos do
    on_arm do
      url "https://github.com/maqalaqil/mhost/releases/download/v#{version}/mhost-aarch64-apple-darwin"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/maqalaqil/mhost/releases/download/v#{version}/mhost-x86_64-apple-darwin"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/maqalaqil/mhost/releases/download/v#{version}/mhost-aarch64-unknown-linux-musl"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/maqalaqil/mhost/releases/download/v#{version}/mhost-x86_64-unknown-linux-musl"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "mhost-#{Hardware::CPU.arch}-#{RUBY_PLATFORM.split("-")[0]}" => "mhost"
    bin.install "mhostd-#{Hardware::CPU.arch}-#{RUBY_PLATFORM.split("-")[0]}" => "mhostd"

    # Generate shell completions
    bash_completion.install_symlink doc/"completions/mhost.bash" => "mhost" if File.exist?("doc/completions/mhost.bash")
    zsh_completion.install_symlink doc/"completions/_mhost" => "_mhost" if File.exist?("doc/completions/_mhost")
    fish_completion.install_symlink doc/"completions/mhost.fish" => "mhost.fish" if File.exist?("doc/completions/mhost.fish")
  end

  test do
    assert_match "mhost", shell_output("#{bin}/mhost --version")
  end
end
