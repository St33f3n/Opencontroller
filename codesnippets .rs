fn thing(){
    egui::ComboBox::from_id_salt("12345").selected_text(format!("{}",self.select_test)).show_ui(ui,|ui|{
    let options = ["Test", "String", "Selcetion", "from", "vec"];
    let options: Vec<String>= options.iter().map(|slice| String::from(*slice)).collect();
    for option in options{
        ui.selectable_value(&mut self.select_test, option.clone(), option);
    }
});
}