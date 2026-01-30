import { useState } from "react";
import { Toaster } from "sonner";
import "./App.css";
import Footer from "./components/footer/Footer";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";

function App() {
  const [currentSection, setCurrentSection] = useState<SidebarSection>("general");

  const ActiveComponent = SECTIONS_CONFIG[currentSection].component;

  return (
    <div
      className="h-screen flex flex-col select-none cursor-default bg-background text-text overflow-hidden"
      data-tauri-drag-region="false"
    >
      <Toaster
        theme="system"
        toastOptions={{
          unstyled: true,
          classNames: {
            toast:
              "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm text-text",
            title: "font-medium",
            description: "text-mid-gray",
          },
        }}
      />
      
      {/* Main content area */}
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        
        {/* Scrollable content area */}
        <div className="flex-1 flex flex-col overflow-hidden relative">
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center p-8 gap-4 min-h-full w-full">
              <ActiveComponent />
            </div>
          </div>
        </div>
      </div>
      
      {/* Fixed footer */}
      <Footer />
    </div>
  );
}

export default App;
