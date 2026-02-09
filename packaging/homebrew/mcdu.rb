class Mcdu < Formula
  desc "Modern disk usage analyzer with terminal UI, inspired by ncdu"
  homepage "https://github.com/mikalv/mcdu"
  url "https://github.com/mikalv/mcdu/archive/refs/tags/v#{version}.tar.gz"
  # sha256 "UPDATE_SHA256_ON_RELEASE"
  license "MIT"
  head "https://github.com/mikalv/mcdu.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "mcdu", shell_output("#{bin}/mcdu --help 2>&1", 2)
  end
end
